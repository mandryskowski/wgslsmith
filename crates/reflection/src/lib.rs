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

            let init = input_data
                .get(&format!("{group}:{binding}"))
                .cloned()
                .map(|mut init| {
                    if init.len() < type_desc.buffer_size() as usize {
                        init.resize(type_desc.buffer_size() as usize, 0);
                    }
                    init
                });

            let size = init
                .as_ref()
                .map(|it| it.len() as u32)
                .unwrap_or(0)
                .max(type_desc.size());

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

    (PipelineDescription { resources, enables }, types)
}
