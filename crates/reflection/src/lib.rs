use ast::{Module, StorageClass, VarQualifier};
pub use types::{
    EnableExtension, PipelineDescription, PipelineResource, ResourceData, ResourceKind,
};

pub fn reflect(
    module: &Module,
    input_data: &std::collections::HashMap<String, Vec<u8>>,
) -> (PipelineDescription, Vec<common::Type>) {
    let mut resources = vec![];
    let mut types = vec![];

    for var in &module.vars {
        if let Some(VarQualifier { storage_class, .. }) = &var.qualifier {
            let kind = match storage_class {
                StorageClass::Uniform => ResourceKind::UniformBuffer,
                StorageClass::Storage => ResourceKind::StorageBuffer,
                _ => continue,
            };

            let type_desc =
                common::Type::try_from(&var.data_type).expect("invalid type for pipeline resource");

            let group = var
                .group_index()
                .expect("resource variable must have group attribute");

            let binding = var
                .binding_index()
                .expect("resource variable must have binding attribute");

            let mut min_size = type_desc.buffer_size();
            min_size = (min_size + 3) & !3;

            let init = input_data
                .get(&format!("{group}:{binding}"))
                .cloned()
                .map(|mut init| {
                    if init.len() < min_size as usize {
                        init.resize(min_size as usize, 0);
                    }
                    init
                });

            let size = init.as_ref().map(|it| it.len() as u32).unwrap_or(min_size);

            resources.push(PipelineResource {
                name: var.name.clone(),
                kind,
                group,
                binding,
                init,
                size,
            });

            types.push(type_desc);
        }
    }

    let enables = module
        .enables
        .iter()
        .map(|e| match e {
            ast::EnableExtension::F16 => types::EnableExtension::F16,
        })
        .collect();

    let entry_point = module
        .functions
        .iter()
        .find(|f| {
            f.attrs
                .iter()
                .any(|attr| matches!(attr, ast::FnAttr::Stage(ast::ShaderStage::Compute)))
        })
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "main".to_owned());

    (
        PipelineDescription {
            resources,
            enables,
            entry_point,
        },
        types,
    )
}
