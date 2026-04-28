use ast::types::{DataType, MemoryViewType, ScalarType};
use ast::*;
use concretizer::concretizer::{Concretizer, ErrorHandling, Options};
use concretizer::value::Value;
use peeking_take_while::PeekableExt;
use pest::iterators::Pair;
use pest::pratt_parser::{Assoc, Op, PrattParser};
use pest::Parser;
use rpds::HashTrieMap;
use std::hash::Hash;
use std::rc::Rc;
use strum::IntoEnumIterator;

#[derive(pest_derive::Parser)]
#[grammar = "grammar.pest"]
struct WGSLParser;

enum Func {
    Builtin(BuiltinFn),
    User(Vec<DataType>, Option<DataType>),
}

impl Func {
    pub fn return_type<'a>(&self, params: impl Iterator<Item = &'a DataType>) -> Option<DataType> {
        match self {
            Func::Builtin(ty) => ty.return_type(params),
            Func::User(_, return_type) => return_type.clone(),
        }
    }

    pub fn params(&self) -> Option<&[DataType]> {
        match self {
            Func::Builtin(_) => None, // TODO
            Func::User(params, _) => Some(params),
        }
    }
}

#[derive(Clone, Default)]
pub struct Environment {
    vars: HashTrieMap<String, DataType>,
    fns: HashTrieMap<String, Func>,
    types: HashTrieMap<String, DataType>,
    consts: HashTrieMap<String, Value>,
}

fn builtins() -> HashTrieMap<String, Func> {
    BuiltinFn::iter()
        .map(|it| (it.as_ref().to_owned(), Func::Builtin(it)))
        .collect()
}

impl Environment {
    pub fn new() -> Self {
        Environment {
            vars: HashTrieMap::new(),
            fns: builtins(),
            types: HashTrieMap::new(),
            consts: HashTrieMap::new(),
        }
    }

    pub fn var(&self, name: &str) -> Option<&DataType> {
        self.vars.get(name)
    }

    pub fn insert_var(&mut self, name: String, ty: DataType) {
        self.vars.insert_mut(name, ty);
    }

    pub fn ty(&self, name: &str) -> Option<&DataType> {
        self.types.get(name)
    }

    pub fn insert_type(&mut self, name: String, decl: DataType) {
        self.types.insert_mut(name, decl);
    }

    pub fn const_value(&self, name: &str) -> Option<&Value> {
        self.consts.get(name)
    }

    pub fn insert_const(&mut self, name: String, val: Value) {
        self.consts.insert_mut(name, val);
    }

    pub fn iter_consts(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.consts.iter()
    }

    pub fn func<'a>(
        &self,
        name: &str,
        params: impl Iterator<Item = &'a DataType>,
    ) -> Option<Option<DataType>> {
        self.fns.get(name).map(|it| it.return_type(params))
    }

    pub fn func_params(&self, name: &str) -> Option<&[DataType]> {
        self.fns.get(name).and_then(|it| it.params())
    }

    pub fn insert_func(&mut self, name: String, params: Vec<DataType>, ret_ty: Option<DataType>) {
        self.fns.insert_mut(name, Func::User(params, ret_ty));
    }
}

pub fn parse(input: &str) -> Module {
    let pairs = WGSLParser::parse(Rule::translation_unit, input).unwrap();
    let pair = pairs.into_iter().next().unwrap();
    parse_translation_unit(pair, &mut Environment::new())
}

fn get_defined_name(pair: &Pair<Rule>) -> Option<String> {
    let inner = pair.clone().into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::enable_directive | Rule::requires_directive | Rule::global_const_assert => None,
        Rule::type_alias_decl
        | Rule::struct_decl
        | Rule::global_constant_decl
        | Rule::global_override_decl
        | Rule::global_variable_decl
        | Rule::function_decl => inner
            .into_inner()
            .find(|p| p.as_rule() == Rule::ident)
            .map(|p| p.as_str().to_owned()),
        _ => None,
    }
}

fn extract_references(
    pair: Pair<Rule>,
    refs: &mut Vec<String>,
    local_scopes: &mut Vec<Vec<String>>,
) {
    match pair.as_rule() {
        Rule::compound_statement | Rule::for_statement => {
            local_scopes.push(Vec::new());
            for child in pair.into_inner() {
                extract_references(child, refs, local_scopes);
            }
            local_scopes.pop();
        }
        Rule::function_decl => {
            local_scopes.push(Vec::new());
            for child in pair.into_inner() {
                if child.as_rule() != Rule::ident {
                    extract_references(child, refs, local_scopes);
                }
            }
            local_scopes.pop();
        }
        Rule::param => {
            let mut ident = None;
            for child in pair.into_inner() {
                if child.as_rule() == Rule::ident {
                    ident = Some(child.as_str().to_owned());
                } else if child.as_rule() == Rule::type_decl
                    || child.as_rule() == Rule::attribute_list
                {
                    extract_references(child, refs, local_scopes);
                }
            }
            if let Some(id) = ident {
                if let Some(scope) = local_scopes.last_mut() {
                    scope.push(id);
                }
            }
        }
        Rule::var_statement | Rule::let_statement | Rule::const_statement => {
            let mut ident = None;
            for child in pair.into_inner() {
                if child.as_rule() == Rule::ident {
                    ident = Some(child.as_str().to_owned());
                } else if child.as_rule() == Rule::type_decl || child.as_rule() == Rule::expression
                {
                    extract_references(child, refs, local_scopes);
                }
            }
            if let Some(id) = ident {
                if let Some(scope) = local_scopes.last_mut() {
                    scope.push(id);
                }
            }
        }
        Rule::struct_member => {
            for child in pair.into_inner() {
                if child.as_rule() == Rule::type_decl || child.as_rule() == Rule::attribute_list {
                    extract_references(child, refs, local_scopes);
                }
            }
        }
        Rule::global_constant_decl
        | Rule::global_override_decl
        | Rule::global_variable_decl
        | Rule::struct_decl => {
            for child in pair.into_inner() {
                if child.as_rule() != Rule::ident {
                    extract_references(child, refs, local_scopes);
                }
            }
        }
        Rule::type_alias_decl => {
            for child in pair.into_inner() {
                if child.as_rule() == Rule::type_decl {
                    extract_references(child, refs, local_scopes);
                }
            }
        }
        Rule::postfix_expression => {
            for child in pair.into_inner() {
                if child.as_rule() == Rule::expression {
                    extract_references(child, refs, local_scopes);
                }
            }
        }
        Rule::ident => {
            let name = pair.as_str().to_owned();
            let is_local = local_scopes.iter().any(|scope| scope.contains(&name));
            if !is_local {
                refs.push(name);
            }
        }
        _ => {
            for child in pair.into_inner() {
                extract_references(child, refs, local_scopes);
            }
        }
    }
}

fn parse_translation_unit(pair: Pair<Rule>, env: &mut Environment) -> Module {
    let decl_pairs: Vec<_> = pair
        .into_inner()
        .take_while(|pair| pair.as_rule() != Rule::EOI)
        .collect();

    // Out of order declarations
    struct DeclNode<'a> {
        pair: Pair<'a, Rule>,
        defined_name: Option<String>,
        referenced_names: Vec<String>,
    }

    let mut nodes = Vec::new();
    for p in decl_pairs {
        let defined_name = get_defined_name(&p);
        let mut referenced_names = Vec::new();

        let inner = p.clone().into_inner().next().unwrap();
        extract_references(inner, &mut referenced_names, &mut Vec::new());

        nodes.push(DeclNode {
            pair: p,
            defined_name,
            referenced_names,
        });
    }

    let num_nodes = nodes.len();
    let mut adj = vec![vec![]; num_nodes];
    let mut in_degree = vec![0; num_nodes];

    for i in 0..num_nodes {
        for j in 0..num_nodes {
            if i == j {
                continue;
            }
            if let Some(ref def_name) = nodes[j].defined_name {
                if nodes[i].referenced_names.contains(def_name) {
                    adj[j].push(i);
                    in_degree[i] += 1;
                }
            }
        }
    }

    // topological sort
    let mut sorted_pairs = Vec::new();
    let mut processed = vec![false; num_nodes];

    for _ in 0..num_nodes {
        let mut found = false;
        for i in 0..num_nodes {
            if !processed[i] && in_degree[i] == 0 {
                sorted_pairs.push(nodes[i].pair.clone());
                processed[i] = true;
                for &v in &adj[i] {
                    in_degree[v] -= 1;
                }
                found = true;
                break;
            }
        }
        if !found {
            panic!("recursive declaration detected at module scope");
        }
    }

    let decls = sorted_pairs
        .into_iter()
        .map(|pair| parse_global_decl(pair, env))
        .collect::<Vec<_>>();

    let mut enables = vec![];
    let mut requires = vec![];
    let mut aliases = vec![];
    let mut functions = vec![];
    let mut structs = vec![];
    let mut consts = vec![];
    let mut overrides = vec![];
    let mut vars = vec![];
    let mut const_asserts = vec![];

    for decl in decls {
        match decl {
            GlobalDecl::Enable(decl) => enables.push(decl),
            GlobalDecl::Requires(decl) => requires.push(decl),
            GlobalDecl::Alias(decl) => aliases.push(decl),
            GlobalDecl::Const(decl) => consts.push(decl),
            GlobalDecl::Override(decl) => overrides.push(decl),
            GlobalDecl::Var(decl) => vars.push(decl),
            GlobalDecl::Struct(decl) => structs.push(decl),
            GlobalDecl::Fn(decl) => functions.push(decl),
            GlobalDecl::ConstAssert(decl) => const_asserts.push(decl),
        }
    }

    Module {
        enables,
        requires,
        aliases,
        functions,
        structs,
        consts,
        overrides,
        vars,
        const_asserts,
    }
}

enum GlobalDecl {
    Enable(ast::EnableExtension),
    Requires(ast::RequiresExtension),
    Alias(AliasDecl),
    Const(ConstDeclStatement),
    Override(OverrideDecl),
    Var(GlobalVarDecl),
    Struct(Rc<StructDecl>),
    Fn(FnDecl),
    ConstAssert(ConstAssertStatement),
}

fn parse_global_decl(pair: Pair<Rule>, env: &mut Environment) -> GlobalDecl {
    let pair = pair.into_inner().next().unwrap();
    match pair.as_rule() {
        Rule::enable_directive => GlobalDecl::Enable(parse_enable_directive(pair)),
        Rule::requires_directive => GlobalDecl::Requires(parse_requires_directive(pair)),
        Rule::type_alias_decl => GlobalDecl::Alias(parse_alias_decl(pair, env)),
        Rule::global_constant_decl => GlobalDecl::Const(parse_global_const_decl(pair, env)),
        Rule::global_override_decl => GlobalDecl::Override(parse_global_override_decl(pair, env)),
        Rule::global_variable_decl => GlobalDecl::Var(parse_global_variable_decl(pair, env)),
        Rule::struct_decl => GlobalDecl::Struct(parse_struct_decl(pair, env)),
        Rule::function_decl => GlobalDecl::Fn(parse_function_decl(pair, env)),
        Rule::global_const_assert => GlobalDecl::ConstAssert(parse_global_const_assert(pair, env)),
        _ => unreachable!(),
    }
}

fn parse_global_const_assert(pair: Pair<Rule>, env: &Environment) -> ConstAssertStatement {
    let mut pairs = pair.into_inner();
    let expr = parse_expression(pairs.next().unwrap(), env, Some(&ScalarType::Bool.into()));
    ConstAssertStatement::new(expr)
}

fn parse_enable_directive(pair: Pair<Rule>) -> ast::EnableExtension {
    match pair.into_inner().next().unwrap().as_str() {
        "f16" => ast::EnableExtension::F16,
        "subgroups" => ast::EnableExtension::Subgroups,
        ext => panic!("unsupported enable extension: {}", ext),
    }
}

fn parse_requires_directive(pair: Pair<Rule>) -> ast::RequiresExtension {
    match pair.into_inner().next().unwrap().as_str() {
        "texel_buffers" => ast::RequiresExtension::TexelBuffers,
        ext => panic!("unsupported requires extension: {}", ext),
    }
}

fn parse_global_const_decl(pair: Pair<Rule>, env: &mut Environment) -> ConstDeclStatement {
    let mut pairs = pair.into_inner().peekable();

    let ident = pairs.next().unwrap().as_str().to_owned();
    let mut data_type = None;

    if let Some(pair) = pairs.peek() {
        if pair.as_rule() == Rule::type_decl {
            let pair = pairs.next().unwrap();
            data_type = Some(parse_type_decl(pair, env));
        }
    }

    let expr = parse_expression(pairs.next().unwrap(), env, data_type.as_ref());
    let data_type = data_type.unwrap_or_else(|| expr.data_type.clone());

    env.insert_var(ident.clone(), data_type.clone());

    let mut concretizer = Concretizer::new(Options {
        error_handling: ErrorHandling::Panic,
    });
    for (k, v) in env.iter_consts() {
        concretizer.register_const(k.clone(), v.clone());
    }

    let val = concretizer.concretize_expr(expr.clone()).value;
    if let Some(val) = val {
        env.insert_const(ident.clone(), val);
    }

    ConstDeclStatement {
        ident,
        data_type: Some(data_type),
        initializer: expr,
    }
}

fn parse_global_override_decl(pair: Pair<Rule>, env: &mut Environment) -> OverrideDecl {
    let mut pairs = pair.into_inner().peekable();

    let attrs = pairs
        .by_ref()
        .peeking_take_while(|pair| pair.as_rule() == Rule::attribute_list)
        .flat_map(|pair| {
            pair.into_inner().map(|pair| {
                let mut pairs = pair.into_inner();
                let name = pairs.next().unwrap().as_str();
                let arg = pairs.next().unwrap().as_str();
                match name {
                    "id" => OverrideAttr::Id(arg.parse().unwrap()),
                    _ => panic!("invalid override attribute: {}", name),
                }
            })
        })
        .collect();

    let name = pairs.next().unwrap().as_str().to_owned();
    let mut data_type = None;
    let mut expr = None;

    if let Some(pair) = pairs.peek() {
        if pair.as_rule() == Rule::type_decl {
            let pair = pairs.next().unwrap();
            data_type = Some(parse_type_decl(pair, env));
        }
    }

    if let Some(pair) = pairs.peek() {
        if pair.as_rule() == Rule::expression {
            let pair = pairs.next().unwrap();
            expr = Some(parse_expression(pair, env, data_type.as_ref()));
        }
    }

    let inferred_type = data_type.clone().unwrap_or_else(|| {
        expr.as_ref()
            .expect("override must have type or initializer")
            .data_type
            .clone()
    });

    env.insert_var(name.clone(), inferred_type);

    if let Some(e) = &expr {
        let mut concretizer = Concretizer::new(Options {
            error_handling: ErrorHandling::Panic,
        });
        for (k, v) in env.iter_consts() {
            concretizer.register_const(k.clone(), v.clone());
        }

        let val = concretizer.concretize_expr(e.clone()).value;
        if let Some(val) = val {
            env.insert_const(name.clone(), val);
        }
    }

    OverrideDecl {
        attrs,
        name,
        data_type,
        initializer: expr,
    }
}

fn parse_global_variable_decl(pair: Pair<Rule>, env: &mut Environment) -> GlobalVarDecl {
    let mut pairs = pair.into_inner().peekable();

    let attrs = pairs
        .by_ref()
        .peeking_take_while(|pair| pair.as_rule() == Rule::attribute_list)
        .flat_map(|pair| {
            pair.into_inner().map(|pair| {
                let mut pairs = pair.into_inner();
                let name = pairs.next().unwrap().as_str();
                let arg = pairs.next().unwrap().as_str();
                match name {
                    "binding" => GlobalVarAttr::Binding(arg.parse().unwrap()),
                    "group" => GlobalVarAttr::Group(arg.parse().unwrap()),
                    _ => panic!("invalid global variable attribute: {}", name),
                }
            })
        })
        .collect();

    let mut qualifier = None;

    if let Some(pair) = pairs.peek() {
        if pair.as_rule() == Rule::variable_qualifier {
            let mut pairs = pairs.next().unwrap().into_inner();
            let storage_class = parse_storage_class(pairs.next().unwrap());

            let access_mode = if matches!(pairs.peek(), Some(access_mode) if access_mode.as_rule() == Rule::access_mode)
            {
                Some(parse_access_mode(pairs.next().unwrap().as_str()))
            } else {
                None
            };

            qualifier = Some(VarQualifier {
                storage_class,
                access_mode,
            })
        }
    }

    let name = pairs.next().unwrap().as_str().to_owned();
    let mut data_type = None;
    let mut expr = None;

    if let Some(pair) = pairs.peek() {
        if pair.as_rule() == Rule::type_decl {
            let pair = pairs.next().unwrap();
            data_type = Some(parse_type_decl(pair, env));
        }
    }

    if pairs.peek().is_some() {
        let pair = pairs.next().unwrap();
        expr = Some(parse_expression(pair, env, data_type.as_ref()))
    }

    let data_type = data_type.unwrap_or_else(|| {
        expr.as_ref()
            .expect("var declaration must have type or initializer")
            .data_type
            .clone()
    });

    let qualifier = if let Some(q) = qualifier {
        q
    } else if matches!(data_type, DataType::Texture(_) | DataType::Sampler(_)) {
        VarQualifier {
            storage_class: StorageClass::Handle,
            access_mode: None,
        }
    } else {
        panic!("module scope var declaration must specify storage class")
    };

    let mut ref_view = MemoryViewType::new(data_type.clone(), qualifier.storage_class);
    if let Some(access_mode) = qualifier.access_mode {
        ref_view.access_mode = access_mode;
    }

    env.insert_var(name.clone(), DataType::Ref(ref_view));

    GlobalVarDecl {
        attrs,
        qualifier: Some(qualifier),
        name,
        data_type,
        initializer: expr,
    }
}

fn parse_alias_decl(pair: Pair<Rule>, env: &mut Environment) -> AliasDecl {
    let mut pairs = pair.into_inner();
    let name = pairs.next().unwrap().as_str().to_owned();
    let data_type = parse_type_decl(pairs.next().unwrap(), env);

    env.insert_type(name.clone(), data_type.clone());

    AliasDecl { name, data_type }
}

fn parse_struct_decl(pair: Pair<Rule>, env: &mut Environment) -> Rc<StructDecl> {
    let mut pairs = pair.into_inner();
    let name = pairs.next().unwrap().as_str().to_owned();
    let members = pairs
        .map(|pair| {
            let mut pairs = pair.into_inner().peekable();

            let attrs = pairs
                .by_ref()
                .peeking_take_while(|pair| pair.as_rule() == Rule::attribute_list)
                .flat_map(|pair| {
                    pair.into_inner().map(|pair| {
                        let mut pairs = pair.into_inner();
                        let name = pairs.next().unwrap().as_str();
                        match name {
                            "align" => StructMemberAttr::Align(
                                pairs.next().unwrap().as_str().parse().unwrap(),
                            ),
                            "blend_src" => StructMemberAttr::BlendSrc(
                                pairs.next().unwrap().as_str().parse().unwrap(),
                            ),
                            "size" => StructMemberAttr::Size(
                                pairs.next().unwrap().as_str().parse().unwrap(),
                            ),
                            "builtin" => StructMemberAttr::Builtin(
                                pairs.next().unwrap().as_str().parse().unwrap(),
                            ),
                            "interpolate" => {
                                let ty = pairs.next().unwrap().as_str().parse().unwrap();
                                let sample = pairs.next().map(|arg| arg.as_str().parse().unwrap());
                                StructMemberAttr::Interpolate(ty, sample)
                            }
                            "invariant" => StructMemberAttr::Invariant,
                            "location" => StructMemberAttr::Location(
                                pairs.next().unwrap().as_str().parse().unwrap(),
                            ),
                            _ => panic!("invalid struct member attribute: {}", name),
                        }
                    })
                })
                .collect();

            let name = pairs.next().unwrap().as_str().to_owned();
            let data_type = parse_type_decl(pairs.next().unwrap(), env);
            StructMember::new(attrs, name, data_type)
        })
        .collect();

    let decl = StructDecl::new(name.clone(), members);
    let params = decl.members.iter().map(|m| m.data_type.clone()).collect();

    env.insert_type(name, DataType::Struct(decl.clone()));
    env.insert_func(
        decl.name.clone(),
        params,
        Some(DataType::Struct(decl.clone())),
    );

    decl
}

fn parse_function_decl(pair: Pair<Rule>, env: &mut Environment) -> FnDecl {
    let mut pairs = pair.into_inner().peekable();

    let mut attrs = Vec::new();
    while let Some(pair) = pairs.peek() {
        if pair.as_rule() == Rule::attribute_list {
            let pair = pairs.next().unwrap();
            for attr_pair in pair.into_inner() {
                let mut attr_pairs = attr_pair.into_inner();
                let name = attr_pairs.next().unwrap().as_str();
                let attr = match name {
                    "compute" => FnAttr::Stage(ShaderStage::Compute),
                    "vertex" => FnAttr::Stage(ShaderStage::Vertex),
                    "fragment" => FnAttr::Stage(ShaderStage::Fragment),
                    "stage" => FnAttr::Stage(match attr_pairs.next().unwrap().as_str().trim() {
                        "compute" => ShaderStage::Compute,
                        "vertex" => ShaderStage::Vertex,
                        "fragment" => ShaderStage::Fragment,
                        _ => panic!("invalid argument for stage attr"),
                    }),
                    "workgroup_size" => FnAttr::WorkgroupSize(
                        attr_pairs
                            .map(|p| parse_expression(p, env, Some(&ScalarType::U32.into())))
                            .collect(),
                    ),
                    "must_use" => FnAttr::MustUse,
                    _ => panic!("invalid function attribute: {}", name),
                };
                attrs.push(attr);
            }
        } else {
            break;
        }
    }

    let name = pairs.next().unwrap().as_str().to_owned();
    let inputs = pairs
        .by_ref()
        .peeking_take_while(|pair| pair.as_rule() == Rule::param)
        .map(|pair| {
            let mut pairs = pair.into_inner().peekable();

            let attrs = pairs
                .by_ref()
                .peeking_take_while(|pair| pair.as_rule() == Rule::attribute_list)
                .flat_map(|pair| pair.into_inner().map(parse_fn_param_return_attr))
                .collect::<Vec<_>>();

            let name = pairs.next().unwrap().as_str().to_owned();
            let data_type = parse_type_decl(pairs.next().unwrap(), env);

            FnInput {
                attrs,
                name,
                data_type,
            }
        })
        .collect::<Vec<_>>();

    let output_attrs = pairs
        .by_ref()
        .peeking_take_while(|pair| pair.as_rule() == Rule::attribute_list)
        .flat_map(|pair| pair.into_inner().map(parse_fn_param_return_attr))
        .collect::<Vec<_>>();

    let output = pairs
        .by_ref()
        .peeking_take_while(|pair| pair.as_rule() == Rule::type_decl)
        .map(|pair| FnOutput {
            attrs: output_attrs.clone(),
            data_type: parse_type_decl(pair, env),
        })
        .next();

    let param_types = inputs.iter().map(|i| i.data_type.clone()).collect();
    env.insert_func(
        name.clone(),
        param_types,
        output.as_ref().map(|o| o.data_type.clone()),
    );

    let mut env = env.clone();
    for param in &inputs {
        env.insert_var(param.name.clone(), param.data_type.clone());
    }

    let body = parse_compound_statement(pairs.next().unwrap(), &env).into_compount_statement();

    FnDecl {
        attrs,
        name,
        inputs,
        output,
        body,
    }
}

fn parse_fn_param_return_attr(pair: Pair<Rule>) -> FnParamReturnAttr {
    let mut pairs = pair.into_inner();
    let name = pairs.next().unwrap().as_str();

    match name {
        "builtin" => {
            let arg = pairs.next().unwrap().as_str();
            FnParamReturnAttr::Builtin(arg.parse().unwrap())
        }
        "invariant" => FnParamReturnAttr::Invariant,
        "location" => {
            let arg = pairs.next().unwrap().as_str();
            FnParamReturnAttr::Location(arg.parse().unwrap())
        }
        "interpolate" => {
            let ty = pairs.next().unwrap().as_str().parse().unwrap();
            let sample = pairs.next().map(|arg| arg.as_str().parse().unwrap());
            FnParamReturnAttr::Interpolate(ty, sample)
        }
        _ => panic!("invalid param attribute: {}", name),
    }
}

fn parse_statement(pair: Pair<Rule>, env: &mut Environment) -> Statement {
    let pair = if pair.as_rule() == Rule::statement {
        pair.into_inner().next().unwrap()
    } else {
        pair
    };

    match pair.as_rule() {
        Rule::let_statement => parse_let_statement(pair, env),
        Rule::const_statement => parse_const_statement(pair, env),
        Rule::var_statement => parse_var_statement(pair, env),
        Rule::const_assert_statement => parse_const_assert_statement(pair, env),
        Rule::discard_statement => Statement::Discard(DiscardStatement),
        Rule::assignment_statement => parse_assignment_statement(pair, env),
        Rule::compound_statement => parse_compound_statement(pair, env),
        Rule::if_statement => parse_if_statement(pair, env),
        Rule::return_statement => parse_return_statement(pair, env),
        Rule::loop_statement => parse_loop_statement(pair, env),
        Rule::while_statement => parse_while_statement(pair, env),
        Rule::break_statement => Statement::Break,
        Rule::continue_statement => Statement::Continue,
        Rule::fallthrough_statement => Statement::Fallthrough,
        Rule::switch_statement => parse_switch_statement(pair, env),
        Rule::for_statement => parse_for_statement(pair, env),
        Rule::call_statement => parse_call_statement(pair, env),
        Rule::increment_statement => parse_increment_statement(pair, env),
        Rule::decrement_statement => parse_decrement_statement(pair, env),
        Rule::fused_marker_statement => {
            let s = pair.as_str();
            let start_idx =
                s.find("WGSLSMITH_CONTEXT_MARKER_").unwrap() + "WGSLSMITH_CONTEXT_MARKER_".len();
            let ctx = s[start_idx..].trim().to_owned();
            Statement::ContextMarker(ContextMarkerStatement { context: ctx })
        }
        _ => unreachable!(),
    }
}

fn parse_const_assert_statement(pair: Pair<Rule>, env: &Environment) -> Statement {
    let mut pairs = pair.into_inner();
    let expr = parse_expression(pairs.next().unwrap(), env, Some(&ScalarType::Bool.into()));
    ConstAssertStatement::new(expr).into()
}

fn parse_increment_statement(pair: Pair<Rule>, env: &Environment) -> Statement {
    let mut pairs = pair.into_inner();
    let lhs = parse_assignment_lhs(pairs.next().unwrap(), env);
    IncrementStatement::new(lhs).into()
}

fn parse_decrement_statement(pair: Pair<Rule>, env: &Environment) -> Statement {
    let mut pairs = pair.into_inner();
    let lhs = parse_assignment_lhs(pairs.next().unwrap(), env);
    DecrementStatement::new(lhs).into()
}

fn parse_let_statement(pair: Pair<Rule>, env: &mut Environment) -> Statement {
    let mut pairs = pair.into_inner();
    let ident = pairs.next().unwrap().as_str().to_owned();

    let mut pair = pairs.next().unwrap();
    let mut specified_type = None;

    if pair.as_rule() == Rule::type_decl {
        specified_type = Some(parse_type_decl(pair, env));
        pair = pairs.next().unwrap();
    }

    let initializer = parse_expression(pair, env, specified_type.as_ref());
    let stmt = LetDeclStatement::new(ident.clone(), specified_type, initializer);
    env.insert_var(ident, stmt.inferred_type().clone());
    stmt.into()
}

fn parse_const_statement(pair: Pair<Rule>, env: &mut Environment) -> Statement {
    let mut pairs = pair.into_inner();
    let ident = pairs.next().unwrap().as_str().to_owned();

    let mut pair = pairs.next().unwrap();
    let mut specified_type = None;

    if pair.as_rule() == Rule::type_decl {
        specified_type = Some(parse_type_decl(pair, env));
        pair = pairs.next().unwrap();
    }

    let initializer = parse_expression(pair, env, specified_type.as_ref());
    let stmt = ConstDeclStatement::new(ident.clone(), specified_type, initializer);
    env.insert_var(ident.clone(), stmt.inferred_type().clone());

    let mut concretizer = Concretizer::new(Options {
        error_handling: ErrorHandling::Panic,
    });
    for (k, v) in env.iter_consts() {
        concretizer.register_const(k.clone(), v.clone());
    }

    let val = concretizer.concretize_expr(stmt.initializer.clone()).value;
    if let Some(val) = val {
        env.insert_const(ident, val);
    }
    stmt.into()
}

fn parse_var_statement(pair: Pair<Rule>, env: &mut Environment) -> Statement {
    let mut pairs = pair.into_inner();
    let ident = pairs.next().unwrap().as_str().to_owned();

    let mut pair = pairs.next();

    let specified_type = if let Some(Rule::type_decl) = pair.as_ref().map(|it| it.as_rule()) {
        let ty = parse_type_decl(pair.unwrap(), env);
        pair = pairs.next();
        Some(ty)
    } else {
        None
    };

    let initializer = if let Some(Rule::expression) = pair.as_ref().map(|it| it.as_rule()) {
        Some(parse_expression(
            pair.unwrap(),
            env,
            specified_type.as_ref(),
        ))
    } else {
        None
    };

    let stmt = VarDeclStatement::new(ident.clone(), specified_type, initializer);

    let ref_view = MemoryViewType::new(stmt.inferred_type().clone(), StorageClass::Function);
    env.insert_var(ident, DataType::Ref(ref_view));

    stmt.into()
}

fn parse_assignment_statement(pair: Pair<Rule>, env: &Environment) -> Statement {
    let mut pairs = pair.into_inner();

    let lhs = parse_assignment_lhs(pairs.next().unwrap(), env);
    let op = pairs.next().unwrap();
    let expected_type = match &lhs {
        AssignmentLhs::Expr(node) => Some(&node.data_type),
        AssignmentLhs::Phony => None,
    };
    let rhs = parse_expression(pairs.next().unwrap(), env, expected_type);

    let op = op.into_inner().next().unwrap();
    let op = match op.as_rule() {
        Rule::op_assign => AssignmentOp::Simple,
        Rule::compound_assignment_operator => match op.into_inner().next().unwrap().as_rule() {
            Rule::op_plus_equal => AssignmentOp::Plus,
            Rule::op_minus_equal => AssignmentOp::Minus,
            Rule::op_times_equal => AssignmentOp::Times,
            Rule::op_divide_equal => AssignmentOp::Divide,
            Rule::op_mod_equal => AssignmentOp::Mod,
            Rule::op_and_equal => AssignmentOp::And,
            Rule::op_or_equal => AssignmentOp::Or,
            Rule::op_xor_equal => AssignmentOp::Xor,
            Rule::op_lshift_equal => AssignmentOp::LShift,
            Rule::op_rshift_equal => AssignmentOp::RShift,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };

    AssignmentStatement::new(lhs, op, rhs).into()
}

fn parse_assignment_lhs(pair: Pair<Rule>, env: &Environment) -> AssignmentLhs {
    match pair.as_rule() {
        Rule::lhs_phony => AssignmentLhs::Phony,
        Rule::lhs_expression => AssignmentLhs::Expr(parse_lhs_expression(pair, env)),
        _ => unreachable!(),
    }
}

fn parse_compound_statement(pair: Pair<Rule>, env: &Environment) -> Statement {
    let mut inner_env = env.clone();
    Statement::Compound(
        pair.into_inner()
            .map(|pair| parse_statement(pair, &mut inner_env))
            .collect(),
    )
}

fn parse_if_statement(pair: Pair<Rule>, env: &Environment) -> Statement {
    let mut pairs = pair.into_inner();
    let condition = parse_expression(pairs.next().unwrap(), env, Some(&ScalarType::Bool.into()));
    let block = parse_compound_statement(pairs.next().unwrap(), env).into_compount_statement();

    let els = pairs
        .next()
        .map(|pair| match pair.as_rule() {
            Rule::compound_statement => parse_compound_statement(pair, env),
            Rule::if_statement => parse_if_statement(pair, env),
            _ => unreachable!(),
        })
        .map(|stmt| match stmt {
            Statement::Compound(stmts) => Else::Else(stmts),
            Statement::If(stmt) => Else::If(stmt),
            _ => unreachable!(),
        });

    IfStatement::new(condition, block).with_else(els).into()
}

fn parse_return_statement(pair: Pair<Rule>, env: &Environment) -> Statement {
    let expression = pair
        .into_inner()
        .next()
        .map(|pair| parse_expression(pair, env, None));

    if let Some(value) = expression {
        ReturnStatement::new(value).into()
    } else {
        ReturnStatement::none().into()
    }
}

fn parse_loop_statement(pair: Pair<Rule>, env: &Environment) -> Statement {
    let mut inner_env = env.clone();
    let mut pairs = pair.into_inner().peekable();

    let body_stmts = pairs
        .by_ref()
        .peeking_take_while(|pair| pair.as_rule() != Rule::continuing_statement)
        .map(|pair| parse_statement(pair, &mut inner_env))
        .collect();

    let block = Statement::Compound(body_stmts).into_compount_statement();
    // we pass inner_env here since variables defines in the body are valid in continuing.
    let continuing = parse_continuing_statement(pairs.next(), &inner_env);

    LoopStatement::new(block, continuing).into()
}

fn parse_while_statement(pair: Pair<Rule>, env: &Environment) -> Statement {
    let mut pairs = pair.into_inner();
    let condition = parse_expression(pairs.next().unwrap(), env, Some(&ScalarType::Bool.into()));
    let block = parse_compound_statement(pairs.next().unwrap(), env).into_compount_statement();

    WhileStatement::new(condition, block).into()
}

fn parse_continuing_statement(
    pair: Option<Pair<Rule>>,
    env: &Environment,
) -> Option<ContinuingBlock> {
    let pair = pair?;
    if pair.as_rule() != Rule::continuing_statement {
        return None;
    }

    let mut inner_env = env.clone();
    let mut pairs = pair.into_inner().peekable();

    let stmts = pairs
        .by_ref()
        .peeking_take_while(|pair| pair.as_rule() != Rule::break_if_statement)
        .map(|pair| parse_statement(pair, &mut inner_env))
        .collect();
    let break_if = pairs.next().map(|pair| match pair.as_rule() {
        Rule::break_if_statement => parse_expression(
            pair.into_inner().next().unwrap(),
            &inner_env,
            Some(&ScalarType::Bool.into()),
        ),
        _ => unreachable!(),
    });
    Some(ContinuingBlock { stmts, break_if })
}

fn parse_switch_statement(pair: Pair<Rule>, env: &Environment) -> Statement {
    let mut pairs = pair.into_inner();

    let expr = parse_expression(pairs.next().unwrap(), env, None);

    let mut cases = vec![];
    let mut default = None;

    for pair in pairs {
        let mut pairs = pair.into_inner();
        let pair = pairs.next().unwrap();

        if pair.as_rule() == Rule::expression {
            let selector = parse_expression(pair, env, None);
            let body =
                parse_compound_statement(pairs.next().unwrap(), env).into_compount_statement();
            cases.push(SwitchCase { selector, body });
        } else {
            default = Some(parse_compound_statement(pair, env).into_compount_statement());
        }
    }

    let default = default.expect("switch statement must have default case");

    SwitchStatement::new(expr, cases, default).into()
}

fn parse_for_statement(pair: Pair<Rule>, env: &mut Environment) -> Statement {
    let mut pairs = pair.into_inner();

    let mut pair = pairs.next().unwrap();

    let mut init = None;
    if pair.as_rule() == Rule::for_init {
        init = match parse_statement(pair.into_inner().next().unwrap(), env) {
            Statement::VarDecl(stmt) => Some(ForLoopInit::VarDecl(stmt)),
            Statement::LetDecl(stmt) => Some(ForLoopInit::LetDecl(stmt)),
            Statement::ConstDecl(stmt) => Some(ForLoopInit::ConstDecl(stmt)),
            Statement::Assignment(stmt) => Some(ForLoopInit::Assignment(stmt)),
            Statement::Increment(stmt) => Some(ForLoopInit::Increment(stmt)),
            Statement::Decrement(stmt) => Some(ForLoopInit::Decrement(stmt)),
            Statement::FnCall(stmt) => Some(ForLoopInit::Call(stmt)),
            _ => unreachable!(),
        };
        pair = pairs.next().unwrap();
    }

    let mut condition = None;
    if pair.as_rule() == Rule::expression {
        condition = Some(parse_expression(pair, env, Some(&ScalarType::Bool.into())));
        pair = pairs.next().unwrap();
    }

    let mut update = None;
    if pair.as_rule() == Rule::for_update {
        let inner = pair.into_inner().next().unwrap();
        update = match parse_statement(inner, env) {
            Statement::Assignment(stmt) => Some(ForLoopUpdate::Assignment(stmt)),
            Statement::Increment(stmt) => Some(ForLoopUpdate::Increment(stmt)),
            Statement::Decrement(stmt) => Some(ForLoopUpdate::Decrement(stmt)),
            Statement::FnCall(stmt) => Some(ForLoopUpdate::Call(stmt)),
            _ => unreachable!(),
        };
        pair = pairs.next().unwrap();
    }

    let body = parse_compound_statement(pair, env);

    let header = ForLoopHeader {
        init,
        condition,
        update,
    };

    ForLoopStatement::new(header, body.into_compount_statement()).into()
}

fn parse_call_statement(pair: Pair<Rule>, env: &Environment) -> Statement {
    let pair = pair.into_inner().next().unwrap();
    let mut pairs = pair.into_inner();

    let ident_pair = pairs.next().unwrap();
    let ident = ident_pair.as_str().to_owned();

    let params = env.func_params(&ident);
    let args = pairs
        .enumerate()
        .map(|(i, it)| {
            let hint = params.and_then(|p| p.get(i));
            parse_expression(it, env, hint)
        })
        .collect();

    FnCallStatement::new(ident, args).into()
}

fn parse_lhs_expression(pair: Pair<Rule>, env: &Environment) -> LhsExprNode {
    let mut pairs = pair.into_inner().peekable();

    let prefixes: Vec<_> = pairs
        .by_ref()
        .peeking_take_while(|pair| pair.as_rule() != Rule::core_lhs_expression)
        .collect();

    fn parse_core(pair: Pair<Rule>, env: &Environment) -> LhsExprNode {
        let pair = pair.into_inner().next().unwrap();
        match pair.as_rule() {
            Rule::lhs_expression => parse_lhs_expression(pair, env),
            Rule::ident => {
                let ident = pair.as_str().to_owned();
                LhsExprNode {
                    data_type: env
                        .var(&ident)
                        .unwrap_or_else(|| panic!("variable {ident} must be defined before use"))
                        .clone(),
                    expr: LhsExpr::Ident(ident),
                }
            }
            _ => unreachable!(),
        }
    }

    let node = parse_core(pairs.next().unwrap(), env);
    let node = pairs.fold(node, |node, pair| {
        let pair = pair.into_inner().next().unwrap();
        let postfix = match pair.as_rule() {
            Rule::expression => Postfix::Index(Box::new(parse_expression(pair, env, None))),
            Rule::ident => Postfix::Member(pair.as_str().to_owned()),
            _ => unreachable!(),
        };

        LhsExprNode {
            data_type: postfix.type_eval(&node.data_type),
            expr: LhsExpr::Postfix(Box::new(node), postfix),
        }
    });

    prefixes.iter().rev().fold(node, |node, pair| {
        let (data_type, expr) = match pair.as_rule() {
            Rule::op_address_of => (
                UnOp::AddressOf.type_eval(&node.data_type),
                LhsExpr::AddressOf(Box::new(node)),
            ),
            Rule::op_indirection => (
                UnOp::Deref.type_eval(&node.data_type),
                LhsExpr::Deref(Box::new(node)),
            ),
            _ => unreachable!(),
        };

        LhsExprNode { data_type, expr }
    })
}

fn precedence_table() -> PrattParser<Rule> {
    PrattParser::new()
        // Level 1: bitwise operators
        .op(Op::infix(Rule::op_bit_and, Assoc::Left)
            | Op::infix(Rule::op_bit_or, Assoc::Left)
            | Op::infix(Rule::op_bit_xor, Assoc::Left))
        // Level 2: short-circuiting or operator
        .op(Op::infix(Rule::op_log_or, Assoc::Left))
        // Level 3: short-circuiting and operator
        .op(Op::infix(Rule::op_log_and, Assoc::Left))
        // Level 4: comparison operators
        .op(Op::infix(Rule::op_less, Assoc::Left)
            | Op::infix(Rule::op_less_eq, Assoc::Left)
            | Op::infix(Rule::op_greater, Assoc::Left)
            | Op::infix(Rule::op_greater_eq, Assoc::Left)
            | Op::infix(Rule::op_equal, Assoc::Left)
            | Op::infix(Rule::op_nequal, Assoc::Left))
        // Level 5: shift operators
        .op(Op::infix(Rule::op_lshift, Assoc::Left) | Op::infix(Rule::op_rshift, Assoc::Left))
        // Level 6: additive operators
        .op(Op::infix(Rule::op_plus, Assoc::Left) | Op::infix(Rule::op_minus, Assoc::Left))
        // Level 7: multiplicative operators
        .op(Op::infix(Rule::op_times, Assoc::Left)
            | Op::infix(Rule::op_divide, Assoc::Left)
            | Op::infix(Rule::op_mod, Assoc::Left))
}

fn parse_expression(
    pair: Pair<Rule>,
    env: &Environment,
    expected_type: Option<&DataType>,
) -> ExprNode {
    let pairs = pair.into_inner();

    let primary = |pair| parse_unary_expression(pair, env, expected_type);

    // HACKHACK: coerce literal types to concrete types while we do not have abstract numerics
    let infix = |mut l: ExprNode, op: Pair<Rule>, mut r: ExprNode| -> ExprNode {
        let op_rule = op.as_rule();

        if op_rule == Rule::op_lshift || op_rule == Rule::op_rshift {
            if r.data_type.as_scalar() == Some(ScalarType::I32) {
                coerce_type(&mut r, ScalarType::U32);
            }
        } else {
            let l_scalar = l.data_type.as_scalar();
            let r_scalar = r.data_type.as_scalar();

            if let (Some(l_s), Some(r_s)) = (l_scalar, r_scalar) {
                if l_s != r_s {
                    if matches!(l_s, ScalarType::F32 | ScalarType::F16)
                        && matches!(r_s, ScalarType::I32 | ScalarType::U32)
                    {
                        coerce_type(&mut r, l_s);
                    } else if matches!(r_s, ScalarType::F32 | ScalarType::F16)
                        && matches!(l_s, ScalarType::I32 | ScalarType::U32)
                    {
                        coerce_type(&mut l, r_s);
                    } else if l_s == ScalarType::U32 && r_s == ScalarType::I32 {
                        coerce_type(&mut r, ScalarType::U32);
                    } else if r_s == ScalarType::U32 && l_s == ScalarType::I32 {
                        coerce_type(&mut l, ScalarType::U32);
                    }
                }
            }
        }

        BinOpExpr::new(op_rule.into(), l, r).into()
    };

    precedence_table()
        .map_primary(primary)
        .map_infix(infix)
        .parse(pairs)
}

fn coerce_type(node: &mut ExprNode, target_scalar: ScalarType) {
    if let Some(scalar) = node.data_type.as_scalar() {
        if scalar == target_scalar {
            return;
        }

        // Update the type container
        match &mut node.data_type {
            DataType::Scalar(s) => *s = target_scalar,
            DataType::Vector(_, s) => *s = target_scalar,
            DataType::Matrix(_, _, s) => *s = target_scalar,
            _ => {}
        }

        // Rewrite the literal
        match &mut node.expr {
            Expr::Lit(Lit::I32(v)) => match target_scalar {
                ScalarType::U32 => node.expr = Expr::Lit(Lit::U32(*v as u32)),
                ScalarType::F32 => node.expr = Expr::Lit(Lit::F32(*v as f32)),
                ScalarType::F16 => node.expr = Expr::Lit(Lit::F16(half::f16::from_f32(*v as f32))),
                _ => {}
            },
            Expr::Lit(Lit::U32(v)) => match target_scalar {
                ScalarType::I32 => node.expr = Expr::Lit(Lit::I32(*v as i32)),
                ScalarType::F32 => node.expr = Expr::Lit(Lit::F32(*v as f32)),
                ScalarType::F16 => node.expr = Expr::Lit(Lit::F16(half::f16::from_f32(*v as f32))),
                _ => {}
            },
            Expr::BinOp(binop) => {
                coerce_type(&mut binop.left, target_scalar);
                coerce_type(&mut binop.right, target_scalar);
            }
            Expr::UnOp(unop) => {
                coerce_type(&mut unop.inner, target_scalar);
            }
            Expr::TypeCons(tc) => {
                tc.data_type = node.data_type.clone();
                for arg in &mut tc.args {
                    coerce_type(arg, target_scalar);
                }
            }
            Expr::Postfix(pf) => {
                coerce_type(&mut pf.inner, target_scalar);
            }
            _ => {} // Var and FnCall just get their data_type updated
        }
    }
}

fn parse_unary_expression(
    pair: Pair<Rule>,
    env: &Environment,
    expected_type: Option<&DataType>,
) -> ExprNode {
    let mut pairs = pair.into_inner();

    let first_pair = pairs.next().unwrap();
    let op = match first_pair.as_rule() {
        Rule::singular_expression => {
            return parse_singular_expression(first_pair, env, expected_type)
        }
        _ => first_pair,
    };

    let op = match op.as_rule() {
        Rule::op_minus => UnOp::Neg,
        Rule::op_log_not => UnOp::Not,
        Rule::op_bit_not => UnOp::BitNot,
        Rule::op_address_of => UnOp::AddressOf,
        Rule::op_indirection => UnOp::Deref,
        _ => unreachable!(),
    };

    let expr = parse_unary_expression(pairs.next().unwrap(), env, expected_type);

    UnOpExpr::new(op, expr).into()
}

fn parse_singular_expression(
    pair: Pair<Rule>,
    env: &Environment,
    expected_type: Option<&DataType>,
) -> ExprNode {
    let mut pairs = pair.into_inner();
    let mut expr = parse_primary_expression(pairs.next().unwrap(), env, expected_type);

    for pf in pairs {
        let pair = pf.into_inner().next().unwrap();
        let pf = match pair.as_rule() {
            Rule::expression => Postfix::Index(Box::new(parse_expression(pair, env, None))),
            Rule::ident => Postfix::Member(pair.as_str().to_owned()),
            _ => unreachable!(),
        };

        expr = PostfixExpr::new(expr, pf).into();
    }

    expr
}

fn parse_primary_expression(
    pair: Pair<Rule>,
    env: &Environment,
    expected_type: Option<&DataType>,
) -> ExprNode {
    let pair = pair.into_inner().next().unwrap();
    match pair.as_rule() {
        Rule::literal_expression => parse_literal_expression(pair, expected_type),
        Rule::type_cons_expression => parse_type_cons_expression(pair, env, expected_type),
        Rule::call_expression => parse_call_expression(pair, env),
        Rule::bitcast_expression => parse_bitcast_expression(pair, env),
        Rule::var_expression => parse_var_expression(pair, env),
        Rule::paren_expression => parse_paren_expression(pair, env, expected_type),
        Rule::unary_expression => parse_unary_expression(pair, env, expected_type),
        _ => unreachable!(),
    }
}

fn parse_literal_expression(pair: Pair<Rule>, expected_type: Option<&DataType>) -> ExprNode {
    let pair = pair.into_inner().next().unwrap();
    let (t, lit) = match pair.as_rule() {
        Rule::bool_literal => (ScalarType::Bool, Lit::Bool(pair.as_str().parse().unwrap())),
        Rule::uint_literal => {
            let s = pair.as_str().trim_end_matches('u');
            let is_hex = s.starts_with("0x") || s.starts_with("0X");
            let val = if is_hex {
                u32::from_str_radix(&s[2..], 16).unwrap()
            } else {
                s.parse().unwrap()
            };
            (ScalarType::U32, Lit::U32(val))
        }
        Rule::int_literal => {
            let s = pair.as_str();
            let is_hex = s.contains("0x") || s.contains("0X");

            let parse_i32 = |s: &str| -> i32 {
                if is_hex {
                    let (s, neg) = if let Some(stripped) = s.strip_prefix('-') {
                        (stripped, true)
                    } else {
                        (s, false)
                    };
                    let s = s.trim_start_matches("0x").trim_start_matches("0X");
                    let val = i64::from_str_radix(s, 16).unwrap();
                    let val = if neg { -val } else { val };
                    val as i32
                } else {
                    s.parse().unwrap()
                }
            };

            let parse_u32 = |s: &str| -> u32 {
                if is_hex {
                    let (s, neg) = if let Some(stripped) = s.strip_prefix('-') {
                        (stripped, true)
                    } else {
                        (s, false)
                    };
                    let s = s.trim_start_matches("0x").trim_start_matches("0X");
                    let val = u32::from_str_radix(s, 16).unwrap();
                    if neg {
                        val.wrapping_neg()
                    } else {
                        val
                    }
                } else {
                    s.parse().unwrap()
                }
            };

            let parse_f32 = |s: &str| -> f32 {
                if is_hex {
                    parse_hex_float(s)
                } else {
                    s.parse().unwrap()
                }
            };

            if s.ends_with('i') {
                (
                    ScalarType::I32,
                    Lit::I32(parse_i32(s.trim_end_matches('i'))),
                )
            } else {
                // No suffix
                match expected_type.and_then(|t| t.as_scalar()) {
                    Some(ScalarType::U32) => (ScalarType::U32, Lit::U32(parse_u32(s))),
                    Some(ScalarType::F32) => (ScalarType::F32, Lit::F32(parse_f32(s))),
                    Some(ScalarType::F16) => {
                        (ScalarType::F16, Lit::F16(half::f16::from_f32(parse_f32(s))))
                    }
                    _ => (ScalarType::I32, Lit::I32(parse_i32(s))),
                }
            }
        }
        Rule::float_literal => {
            let s = pair.as_str();
            let is_hex = s.contains("0x") || s.contains("0X");
            if s.ends_with('h') {
                let val = if is_hex {
                    parse_hex_float(s)
                } else {
                    s.trim_end_matches('h').parse::<f32>().unwrap()
                };
                (ScalarType::F16, Lit::F16(half::f16::from_f32(val)))
            } else {
                let val = if is_hex {
                    parse_hex_float(s)
                } else {
                    s.trim_end_matches('f').parse::<f32>().unwrap()
                };
                (ScalarType::F32, Lit::F32(val))
            }
        }
        _ => unreachable!(),
    };

    ExprNode {
        data_type: DataType::Scalar(t),
        expr: Expr::Lit(lit),
    }
}

#[derive(PartialEq, Debug)]
enum ImplicitTypeKind {
    None,
    Vector(u8),
    Array,
}

fn parse_type_cons_expression(
    pair: Pair<Rule>,
    env: &Environment,
    expected_type: Option<&DataType>,
) -> ExprNode {
    let mut pairs = pair.into_inner().peekable();

    let type_decl_pair = pairs.next().unwrap();

    let implicit_type_kind = detect_implicit_type(&type_decl_pair);

    if implicit_type_kind != ImplicitTypeKind::None {
        return parse_implicit_type_cons(implicit_type_kind, &mut pairs, env, expected_type);
    }

    let data_type = parse_type_decl(type_decl_pair, env);

    let arg_pairs: Vec<_> = pairs.collect();
    let arg_count = arg_pairs.len();

    let args: Vec<_> = arg_pairs
        .into_iter()
        .map(|pair| {
            let expected_arg_type = match &data_type {
                DataType::Scalar(_) => Some(data_type.clone()),
                DataType::Vector(_, scalar) => Some(DataType::Scalar(*scalar)),
                DataType::Matrix(cols, rows, scalar) => {
                    if arg_count == *cols as usize {
                        Some(DataType::Vector(*rows, *scalar))
                    } else {
                        Some(DataType::Scalar(*scalar))
                    }
                }
                DataType::Array(inner_type, _) => Some((**inner_type).clone()),
                _ => None,
            };
            parse_expression(pair, env, expected_arg_type.as_ref())
        })
        .collect();

    // i32::MIN is written as i32(-2147483648) in WGSL
    // For this reason when we encounter i32(-2147483648) we treat is as a Lit
    // Otherwise, parsing and writing i32(-2147483648) would output i32(i32(-2147483648))
    if data_type == DataType::Scalar(ScalarType::I32)
        && args.len() == 1
        && args[0].expr == Expr::Lit(Lit::I32(i32::MIN))
    {
        return Lit::I32(i32::MIN).into();
    }

    TypeConsExpr::new(data_type, args).into()
}

fn parse_implicit_type_cons(
    kind: ImplicitTypeKind,
    pairs: &mut std::iter::Peekable<pest::iterators::Pairs<'_, Rule>>,
    env: &Environment,
    expected_type: Option<&DataType>,
) -> ExprNode {
    let mut args = Vec::new();

    let mut inferred_element_type = None;

    if let Some(expected) = expected_type {
        match kind {
            ImplicitTypeKind::Vector(n) => {
                if let DataType::Vector(expected_n, scalar) = expected {
                    if *expected_n == n {
                        inferred_element_type = Some(DataType::Scalar(*scalar));
                    }
                }
            }
            ImplicitTypeKind::Array => {
                if let DataType::Array(inner, _) = expected {
                    inferred_element_type = Some((**inner).clone());
                }
            }
            ImplicitTypeKind::None => unreachable!(),
        }
    }

    if let Some(first_pair) = pairs.next() {
        let first_arg = parse_expression(first_pair, env, inferred_element_type.as_ref());
        let first_arg_type = first_arg.data_type.clone();
        args.push(first_arg);

        let container_type = match kind {
            ImplicitTypeKind::Vector(n) => {
                let scalar = if let Some(DataType::Scalar(s)) = inferred_element_type {
                    s
                } else {
                    first_arg_type
                        .as_scalar()
                        .expect("constructor argument must be scalar")
                };
                DataType::Vector(n, scalar)
            }
            ImplicitTypeKind::Array => {
                let elem = inferred_element_type.unwrap_or(first_arg_type);
                DataType::Array(Rc::new(elem), None)
            }
            ImplicitTypeKind::None => unreachable!(),
        };

        let element_type = if let DataType::Vector(_, s) = &container_type {
            DataType::Scalar(*s)
        } else if let DataType::Array(inner, _) = &container_type {
            (**inner).clone()
        } else {
            unreachable!()
        };

        for pair in pairs {
            args.push(parse_expression(pair, env, Some(&element_type)));
        }
        let final_type = if let DataType::Array(inner, _) = container_type {
            DataType::Array(inner, Some(args.len() as u32))
        } else {
            container_type
        };

        TypeConsExpr::new(final_type, args).into()
    } else {
        // No arguments
        if let ImplicitTypeKind::Vector(n) = kind {
            TypeConsExpr::new(DataType::Vector(n, ScalarType::I32), vec![]).into()
        } else {
            panic!("cannot infer type from constructor {:?}", kind)
        }
    }
}

fn detect_implicit_type(pair: &Pair<Rule>) -> ImplicitTypeKind {
    if pair.as_rule() != Rule::built_in_type_decl {
        return ImplicitTypeKind::None;
    }

    let inner = pair.clone().into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::t_vector => {
            let t_vector = inner.into_inner().next().unwrap();
            if !t_vector.as_str().contains('<')
                && !t_vector.as_str().ends_with(['i', 'u', 'f', 'h'])
            {
                match t_vector.as_rule() {
                    Rule::t_vec2 => ImplicitTypeKind::Vector(2),
                    Rule::t_vec3 => ImplicitTypeKind::Vector(3),
                    Rule::t_vec4 => ImplicitTypeKind::Vector(4),
                    _ => unreachable!(),
                }
            } else {
                ImplicitTypeKind::None
            }
        }
        Rule::array_type_decl => {
            if inner.clone().into_inner().next().is_none() {
                ImplicitTypeKind::Array
            } else {
                ImplicitTypeKind::None
            }
        }
        _ => ImplicitTypeKind::None,
    }
}

fn parse_call_expression(pair: Pair<Rule>, env: &Environment) -> ExprNode {
    let mut pairs = pair.into_inner();

    let ident_pair = pairs.next().unwrap();
    let ident_str = ident_pair.as_str();

    // get parameter types for hinting
    let params = env.func_params(ident_str);

    let args: Vec<ExprNode> = pairs
        .enumerate()
        .map(|(i, pair)| {
            let hint = params.and_then(|p| p.get(i));
            parse_expression(pair, env, hint)
        })
        .collect();

    struct FunSig<'a>(String, &'a [ExprNode]);

    impl std::fmt::Display for FunSig<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let FunSig(ident, args) = self;
            let mut args = args.iter();
            write!(f, "{ident}(")?;
            if let Some(arg) = args.next() {
                write!(f, "{}", arg.data_type)?;
            }
            for arg in args {
                write!(f, ", {}", arg.data_type)?;
            }
            write!(f, ")")
        }
    }

    let ident_str = ident_str.to_owned();
    let func_result = env.func(&ident_str, args.iter().map(|arg| &arg.data_type));

    if func_result.is_none() {
        // If there is no matching function ident, try parsing as aliased TypeCons
        let ty = env
            .ty(ident_str.as_str())
            .unwrap_or_else(|| panic!("`{}` not found", FunSig(ident_str.clone(), &args)))
            .clone();
        return TypeConsExpr {
            data_type: ty,
            args,
        }
        .into();
    }

    let return_type = func_result.unwrap().unwrap_or_else(|| {
        panic!(
            "function `{}` does not return a value and cannot be used in an expression",
            ident_str
        )
    });

    FnCallExpr::new(ident_str, args).into_node(return_type)
}

fn parse_bitcast_expression(pair: Pair<Rule>, env: &Environment) -> ExprNode {
    let mut pairs = pair.into_inner();

    let type_decl = pairs.next().unwrap();
    let target_type = parse_type_decl(type_decl, env);

    let args: Vec<_> = pairs
        .map(|pair| parse_expression(pair, env, None))
        .collect();

    let call_expr = FnCallExpr {
        ident: "bitcast".to_owned(),
        template_args: vec![target_type.clone()],
        args,
    };

    call_expr.into_node(target_type)
}

fn parse_type_decl(pair: Pair<Rule>, env: &Environment) -> DataType {
    let pair = pair.into_inner().next().unwrap();

    fn parse_t_scalar(pair: Pair<Rule>) -> ScalarType {
        pair.into_inner().next().unwrap().as_rule().into()
    }

    match pair.as_rule() {
        Rule::t_scalar => DataType::Scalar(parse_t_scalar(pair)),
        Rule::t_vector => {
            let t_vector = pair.into_inner().next().unwrap();
            let n = match t_vector.as_rule() {
                Rule::t_vec2 => 2,
                Rule::t_vec3 => 3,
                Rule::t_vec4 => 4,
                _ => unreachable!(),
            };

            let scalar_type = if let Some(inner) = t_vector.clone().into_inner().next() {
                parse_t_scalar(inner)
            } else {
                match t_vector.as_str().chars().last() {
                    Some('i') => ScalarType::I32,
                    Some('u') => ScalarType::U32,
                    Some('f') => ScalarType::F32,
                    Some('h') => ScalarType::F16,
                    _ => panic!("Explicit type required for vector"),
                }
            };

            DataType::Vector(n, scalar_type)
        }
        Rule::t_matrix => {
            let s = pair.as_str();
            let mut pairs = pair.into_inner();

            let c = s.chars().nth(3).unwrap().to_digit(10).unwrap() as u8;
            let r = s.chars().nth(5).unwrap().to_digit(10).unwrap() as u8;

            let scalar_type = match pairs.next() {
                Some(inner) if inner.as_rule() == Rule::t_scalar => parse_t_scalar(inner),
                _ => match s.chars().nth(6) {
                    Some('i') => ScalarType::I32,
                    Some('u') => ScalarType::U32,
                    Some('f') => ScalarType::F32,
                    Some('h') => ScalarType::F16,
                    _ => ScalarType::F32,
                },
            };

            DataType::Matrix(c, r, scalar_type)
        }
        Rule::t_atomic => {
            let inner = pair.into_inner().next().unwrap();
            DataType::Atomic(parse_t_scalar(inner))
        }
        Rule::array_type_decl => {
            let mut pairs = pair.into_inner();
            if let Some(pair) = pairs.next() {
                let ty = Rc::new(parse_type_decl(pair, env));
                let len = pairs.next().map(|it| {
                    let mut concretizer = Concretizer::new(Options {
                        error_handling: ErrorHandling::Panic,
                    });
                    for (k, v) in env.iter_consts() {
                        concretizer.register_const(k.clone(), v.clone());
                    }
                    match concretizer
                        .concretize_expr(parse_expression(it, env, None))
                        .value
                        .unwrap()
                    {
                        Value::Lit(lit) => match lit {
                            Lit::I32(num) => num as u32,
                            Lit::U32(num) => num,
                            _ => panic!(),
                        },
                        _ => panic!(),
                    }
                });
                DataType::Array(ty, len)
            } else {
                panic!("Explicit type required for array");
            }
        }
        Rule::ptr_type_decl => {
            let mut pairs = pair.into_inner();
            let storage_class = parse_storage_class(pairs.next().unwrap());
            let inner = parse_type_decl(pairs.next().unwrap(), env);
            DataType::Ptr(MemoryViewType::new(inner, storage_class))
        }
        Rule::t_texture => {
            let full_str = pair.as_str();
            let mut inner_pairs = pair.into_inner();

            if full_str == "texture_external" {
                DataType::Texture(TextureType::External)
            } else if full_str.starts_with("texture_depth") {
                let keyword = full_str.split_once('<').map(|(k, _)| k).unwrap_or(full_str);

                if keyword == "texture_depth_multisampled_2d" {
                    DataType::Texture(TextureType::Depth {
                        dim: TextureDimension::D2,
                        clone_of_multisampled: true,
                    })
                } else {
                    let suffix = keyword.strip_prefix("texture_depth_").unwrap();
                    let dim = suffix.parse().expect("invalid texture dimension");
                    DataType::Texture(TextureType::Depth {
                        dim,
                        clone_of_multisampled: false,
                    })
                }
            } else if full_str.starts_with("texture_storage") {
                let keyword_end = full_str.find('<').unwrap();
                let keyword_str = &full_str[..keyword_end];
                let suffix = keyword_str.strip_prefix("texture_storage_").unwrap();
                let dim = suffix.parse().expect("invalid texture dimension");

                let format_str = inner_pairs.next().unwrap().as_str();
                let access_str = inner_pairs.next().unwrap().as_str();

                let format = format_str.parse().expect("invalid texel format");

                let access = parse_access_mode(access_str);

                DataType::Texture(TextureType::Storage {
                    dim,
                    format,
                    access,
                })
            } else if full_str.starts_with("texture_multisampled") {
                let scalar_pair = inner_pairs.next().unwrap();
                let scalar = parse_t_scalar(scalar_pair);

                DataType::Texture(TextureType::Multisampled {
                    dim: TextureDimension::D2,
                    derived_type: scalar,
                })
            } else if full_str.starts_with("texture_") {
                let keyword_end = full_str.find('<').unwrap();
                let keyword_str = &full_str[..keyword_end];

                let suffix = keyword_str.strip_prefix("texture_").unwrap();
                let dim = suffix.parse().expect("invalid texture dimension");

                let scalar_pair = inner_pairs.next().unwrap();
                let scalar = parse_t_scalar(scalar_pair);

                DataType::Texture(TextureType::Sampled {
                    dim,
                    derived_type: scalar,
                })
            } else {
                unreachable!("Unknown texture type: {}", full_str)
            }
        }
        Rule::t_sampler => match pair.as_str() {
            "sampler" => DataType::Sampler(SamplerType::Sampler),
            "sampler_comparison" => DataType::Sampler(SamplerType::Comparison),
            _ => unreachable!(),
        },
        Rule::ident => env
            .ty(pair.as_str())
            .unwrap_or_else(|| panic!("type not found: {}", pair.as_str()))
            .clone(),
        _ => panic!("{}", pair),
    }
}

fn parse_var_expression(pair: Pair<Rule>, env: &Environment) -> ExprNode {
    VarExpr::new(pair.as_str()).into_node(
        env.var(pair.as_str())
            .unwrap_or_else(|| panic!("variable `{}` must be defined before use", pair.as_str()))
            .clone(),
    )
}

fn parse_paren_expression(
    pair: Pair<Rule>,
    env: &Environment,
    expected_type: Option<&DataType>,
) -> ExprNode {
    let pair = pair.into_inner().next().unwrap();
    parse_expression(pair, env, expected_type)
}

fn parse_storage_class(pair: Pair<Rule>) -> StorageClass {
    match pair.as_str() {
        "function" => StorageClass::Function,
        "private" => StorageClass::Private,
        "workgroup" => StorageClass::WorkGroup,
        "uniform" => StorageClass::Uniform,
        "storage" => StorageClass::Storage,
        _ => unreachable!(),
    }
}

fn parse_access_mode(s: &str) -> AccessMode {
    match s {
        "read" => AccessMode::Read,
        "write" => AccessMode::Write,
        "read_write" => AccessMode::ReadWrite,
        _ => unreachable!(),
    }
}

impl From<Rule> for BinOp {
    fn from(rule: Rule) -> Self {
        match rule {
            Rule::op_plus => BinOp::Plus,
            Rule::op_minus => BinOp::Minus,
            Rule::op_times => BinOp::Times,
            Rule::op_divide => BinOp::Divide,
            Rule::op_mod => BinOp::Mod,
            Rule::op_log_and => BinOp::LogAnd,
            Rule::op_log_or => BinOp::LogOr,
            Rule::op_bit_and => BinOp::BitAnd,
            Rule::op_bit_or => BinOp::BitOr,
            Rule::op_bit_xor => BinOp::BitXOr,
            Rule::op_lshift => BinOp::LShift,
            Rule::op_rshift => BinOp::RShift,
            Rule::op_equal => BinOp::Equal,
            Rule::op_nequal => BinOp::NotEqual,
            Rule::op_less => BinOp::Less,
            Rule::op_less_eq => BinOp::LessEqual,
            Rule::op_greater => BinOp::Greater,
            Rule::op_greater_eq => BinOp::GreaterEqual,
            _ => unreachable!(),
        }
    }
}

impl From<Rule> for ScalarType {
    fn from(rule: Rule) -> Self {
        match rule {
            Rule::t_bool => ScalarType::Bool,
            Rule::t_i32 => ScalarType::I32,
            Rule::t_u32 => ScalarType::U32,
            Rule::t_f32 => ScalarType::F32,
            Rule::t_f16 => ScalarType::F16,
            _ => unreachable!(),
        }
    }
}

fn parse_hex_float(s: &str) -> f32 {
    let (s, is_negative) = if let Some(stripped) = s.strip_prefix('-') {
        (stripped, true)
    } else {
        (s, false)
    };
    let s = s.trim_end_matches(['f', 'h']);
    let s = &s[2..];
    let (sig, exp_str) = if let Some(idx) = s.find(['p', 'P']) {
        (&s[..idx], &s[idx + 1..])
    } else {
        (s, "0")
    };

    let mut exp_val = exp_str.parse::<i32>().unwrap();
    let mut int_part = String::new();
    for (i, c) in sig.chars().enumerate() {
        if c == '.' {
            let frac_digits = sig.len() - 1 - i;
            exp_val -= (frac_digits as i32) * 4;
        } else {
            int_part.push(c);
        }
    }

    let mut start_idx = 0;
    while start_idx < int_part.len() && int_part.as_bytes()[start_idx] == b'0' {
        start_idx += 1;
    }
    let sig_str = &int_part[start_idx..];

    let mut val = 0.0_f64;
    if !sig_str.is_empty() {
        let take_len = sig_str.len().min(16);
        let truncated = &sig_str[..take_len];
        let parsed = u64::from_str_radix(truncated, 16).unwrap() as f64;
        let skipped_digits = sig_str.len() - take_len;
        exp_val += (skipped_digits as i32) * 4;
        val = parsed * 2.0_f64.powi(exp_val);
    }

    let result = if is_negative { -val } else { val };
    result as f32
}

#[cfg(test)]
mod tests {
    use crate::*;

    macro_rules! test_case {
        ($name:ident) => {
            test_case!($name, $name);
        };
        ($name:ident, $fn:ident) => {
            #[test]
            fn $fn() {
                const SRC: &str = include_str!(concat!("tests/", stringify!($name), ".wgsl"));
                let pairs = WGSLParser::parse(Rule::translation_unit, SRC).unwrap();
                let pair = pairs.into_iter().next().unwrap();
                let module = parse_translation_unit(pair, &mut Environment::new());
                insta::assert_debug_snapshot!(module);
            }
        };
    }

    test_case!(calls);
    test_case!(floats);
    test_case!(loops);
    test_case!(ptrs);
    test_case!(structs);

    test_case!(test_1);
    test_case!(test_2);
    test_case!(test_3);
    test_case!(test_4);
    test_case!(test_5);
    test_case!(subgroups);
    test_case!(textures);
}
