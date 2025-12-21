use common::Type;
use reflection_types::{PipelineDescription, ResourceKind};

pub fn normalize_execution(
    buffers: &[Vec<u8>],
    pipeline_desc: &PipelineDescription,
    type_descs: &[Type],
) -> Vec<u8> {
    let mut canonical_data = Vec::new();

    for (i, (j, _)) in pipeline_desc
        .resources
        .iter()
        .enumerate()
        .filter(|(_, it)| it.kind == ResourceKind::StorageBuffer)
        .enumerate()
    {
        let buffer = &buffers[i];

        for (offset, size) in type_descs[j].ranges() {
            let range = offset..(offset + size);
            canonical_data.extend_from_slice(&buffer[range]);
        }
    }

    canonical_data
}
pub fn compare<'a>(
    mut buffers: impl Iterator<Item = &'a Vec<Vec<u8>>>,
    pipeline_desc: &PipelineDescription,
    type_descs: &[Type],
) -> bool {
    let mut prev_normalized = match buffers.next() {
        Some(b) => normalize_execution(b, pipeline_desc, type_descs),
        None => return true,
    };

    for execution in buffers {
        let current_normalized = normalize_execution(execution, pipeline_desc, type_descs);

        if current_normalized != prev_normalized {
            return false;
        }

        prev_normalized = current_normalized;
    }

    true
}
