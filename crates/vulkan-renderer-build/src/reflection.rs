//! Typed, source-language-independent shader interface reflection.
//!
//! Slang's JSON is a compiler interchange format, not a runtime ABI. This
//! module validates it on the cold path and emits compact records suitable for
//! a renderer or product-specific binary format.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::{Error, Result, ShaderStage};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ShaderScalarType {
    Bool,
    I32,
    U32,
    F32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ShaderIoDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShaderStageIo {
    pub name: String,
    pub direction: ShaderIoDirection,
    pub location: u32,
    pub scalar_type: ShaderScalarType,
    pub rows: u32,
    pub columns: u32,
    pub location_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShaderUniformMember {
    pub name: String,
    pub byte_offset: u32,
    pub byte_size: u32,
    pub scalar_type: ShaderScalarType,
    pub rows: u32,
    pub columns: u32,
    pub array_count: u32,
    pub array_stride: u32,
    pub matrix_stride: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShaderUniformBuffer {
    pub name: String,
    pub register: u32,
    pub byte_size: u32,
    pub members: Vec<ShaderUniformMember>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShaderInterface {
    pub stage_io: Vec<ShaderStageIo>,
    pub uniform_buffers: Vec<ShaderUniformBuffer>,
}

pub fn reflect_shader_interface(
    reflection: &Value,
    entry_point: &str,
    stage: ShaderStage,
) -> Result<ShaderInterface> {
    let entry = reflection
        .get("entryPoints")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries.iter().find(|entry| {
                entry.get("name").and_then(Value::as_str) == Some(entry_point)
                    && entry.get("stage").and_then(Value::as_str) == Some(stage.slang_name())
            })
        })
        .ok_or_else(|| {
            Error::Reflection(format!(
                "missing {stage:?} entry-point reflection for {entry_point:?}"
            ))
        })?;
    let mut stage_io = Vec::new();
    if let Some(parameters) = entry.get("parameters").and_then(Value::as_array) {
        for parameter in parameters {
            collect_stage_io(parameter, ShaderIoDirection::Input, &mut stage_io)?;
        }
    }
    if let Some(result) = entry.get("result") {
        collect_stage_io(result, ShaderIoDirection::Output, &mut stage_io)?;
    }
    stage_io.sort_by(|left, right| {
        (left.direction, left.location, &left.name).cmp(&(
            right.direction,
            right.location,
            &right.name,
        ))
    });
    let mut io_locations = BTreeSet::new();
    for item in &stage_io {
        for offset in 0..item.location_count {
            let location = item.location.checked_add(offset).ok_or_else(|| {
                Error::Reflection(format!(
                    "entry point {entry_point:?} {:?} location range overflows",
                    item.direction
                ))
            })?;
            if !io_locations.insert((item.direction, location)) {
                return Err(Error::Reflection(format!(
                    "entry point {entry_point:?} repeats {:?} location {location}",
                    item.direction
                )));
            }
        }
    }

    let mut uniform_buffers = Vec::new();
    if let Some(parameters) = reflection.get("parameters").and_then(Value::as_array) {
        for parameter in parameters {
            if binding_kind(parameter) == Some("constantBuffer") {
                uniform_buffers.push(parse_uniform_buffer(parameter)?);
            }
        }
    }
    uniform_buffers.sort_by_key(|buffer| buffer.register);
    let mut registers = BTreeSet::new();
    if uniform_buffers
        .iter()
        .any(|buffer| !registers.insert(buffer.register))
    {
        return Err(Error::Reflection(
            "shader reflection repeats a constant-buffer register".to_owned(),
        ));
    }
    Ok(ShaderInterface {
        stage_io,
        uniform_buffers,
    })
}

fn collect_stage_io(
    value: &Value,
    direction: ShaderIoDirection,
    output: &mut Vec<ShaderStageIo>,
) -> Result<()> {
    if value
        .get("type")
        .and_then(|ty| ty.get("kind"))
        .and_then(Value::as_str)
        == Some("struct")
    {
        for field in value
            .get("type")
            .and_then(|ty| ty.get("fields"))
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Reflection("stage-I/O struct has no fields".to_owned()))?
        {
            collect_stage_io(field, direction, output)?;
        }
        return Ok(());
    }
    let expected_kind = match direction {
        ShaderIoDirection::Input => "varyingInput",
        ShaderIoDirection::Output => "varyingOutput",
    };
    if binding_kind(value) != Some(expected_kind) {
        return Ok(());
    }
    let name = required_string(value, "name", "stage-I/O name")?.to_owned();
    let location = binding_u32(value, "index", "stage-I/O location")?;
    let shape = parse_value_shape(
        value
            .get("type")
            .ok_or_else(|| Error::Reflection(format!("stage-I/O {name:?} has no type")))?,
        ArrayLayout::StageIo,
    )?;
    let location_count = shape
        .columns
        .max(1)
        .checked_mul(shape.array_count)
        .ok_or_else(|| Error::Reflection(format!("stage-I/O {name:?} location span overflows")))?;
    output.push(ShaderStageIo {
        name,
        direction,
        location,
        scalar_type: shape.scalar_type,
        rows: shape.rows,
        columns: shape.columns,
        location_count,
    });
    Ok(())
}

fn parse_uniform_buffer(parameter: &Value) -> Result<ShaderUniformBuffer> {
    let name = required_string(parameter, "name", "constant-buffer name")?.to_owned();
    let register = binding_u32(parameter, "index", "constant-buffer register")?;
    let ty = parameter
        .get("type")
        .ok_or_else(|| Error::Reflection(format!("constant buffer {name:?} has no type")))?;
    let element = ty.get("elementType").ok_or_else(|| {
        Error::Reflection(format!("constant buffer {name:?} has no element type"))
    })?;
    let byte_size = ty
        .get("elementVarLayout")
        .and_then(|layout| layout.get("binding"))
        .and_then(|binding| binding.get("size"))
        .and_then(Value::as_u64)
        .map(u32::try_from)
        .transpose()
        .map_err(|_| Error::Reflection(format!("constant buffer {name:?} size exceeds u32")))?
        .unwrap_or_else(|| occupied_struct_bytes(element).unwrap_or(0));
    let mut members = Vec::new();
    collect_uniform_members(element, 0, "", &mut members)?;
    if members.is_empty() {
        return Err(Error::Reflection(format!(
            "constant buffer {name:?} has no typed members"
        )));
    }
    members.sort_by(|left, right| {
        (left.byte_offset, &left.name).cmp(&(right.byte_offset, &right.name))
    });
    let mut names = BTreeSet::new();
    for member in &members {
        if !names.insert(&member.name) {
            return Err(Error::Reflection(format!(
                "constant buffer {name:?} repeats member {:?}",
                member.name
            )));
        }
        let end = member
            .byte_offset
            .checked_add(member.byte_size)
            .ok_or_else(|| {
                Error::Reflection(format!("uniform member {:?} overflows", member.name))
            })?;
        if end > byte_size {
            return Err(Error::Reflection(format!(
                "uniform member {:?} exceeds constant buffer {name:?}",
                member.name
            )));
        }
    }
    Ok(ShaderUniformBuffer {
        name,
        register,
        byte_size,
        members,
    })
}

fn collect_uniform_members(
    ty: &Value,
    base_offset: u32,
    prefix: &str,
    output: &mut Vec<ShaderUniformMember>,
) -> Result<()> {
    if ty.get("kind").and_then(Value::as_str) != Some("struct") {
        return Err(Error::Reflection(
            "constant-buffer element is not a struct".to_owned(),
        ));
    }
    let fields = ty
        .get("fields")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Reflection("constant-buffer struct has no field array".to_owned()))?;
    let skip_wrapper = prefix.is_empty()
        && fields.len() == 1
        && fields[0]
            .get("type")
            .and_then(|field_type| field_type.get("kind"))
            .and_then(Value::as_str)
            == Some("struct");
    for field in fields {
        let field_name = required_string(field, "name", "uniform member name")?;
        let field_offset = binding_u32(field, "offset", "uniform member offset")?;
        let offset = base_offset.checked_add(field_offset).ok_or_else(|| {
            Error::Reflection(format!("uniform member {field_name:?} offset overflows"))
        })?;
        let field_type = field.get("type").ok_or_else(|| {
            Error::Reflection(format!("uniform member {field_name:?} has no type"))
        })?;
        if field_type.get("kind").and_then(Value::as_str) == Some("struct") {
            let next_prefix = if skip_wrapper {
                prefix.to_owned()
            } else if prefix.is_empty() {
                field_name.to_owned()
            } else {
                format!("{prefix}.{field_name}")
            };
            collect_uniform_members(field_type, offset, &next_prefix, output)?;
            continue;
        }
        let name = if prefix.is_empty() {
            field_name.to_owned()
        } else {
            format!("{prefix}.{field_name}")
        };
        let byte_size = binding_u32(field, "size", "uniform member size")?;
        let shape = parse_value_shape(field_type, ArrayLayout::Uniform)?;
        let matrix_stride = if shape.columns > 1 {
            if shape.array_count != 1 {
                return Err(Error::Reflection(format!(
                    "uniform member {name:?} uses an unsupported array of matrices"
                )));
            }
            byte_size
                .checked_div(shape.rows)
                .filter(|stride| *stride != 0)
                .ok_or_else(|| {
                    Error::Reflection(format!("uniform matrix {name:?} has invalid byte size"))
                })?
        } else {
            0
        };
        output.push(ShaderUniformMember {
            name,
            byte_offset: offset,
            byte_size,
            scalar_type: shape.scalar_type,
            rows: shape.rows,
            columns: shape.columns,
            array_count: shape.array_count,
            array_stride: shape.array_stride,
            matrix_stride,
        });
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ValueShape {
    scalar_type: ShaderScalarType,
    rows: u32,
    columns: u32,
    array_count: u32,
    array_stride: u32,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ArrayLayout {
    StageIo,
    Uniform,
}

fn parse_value_shape(ty: &Value, array_layout: ArrayLayout) -> Result<ValueShape> {
    match ty.get("kind").and_then(Value::as_str) {
        Some("scalar") => Ok(ValueShape {
            scalar_type: parse_scalar(ty)?,
            rows: 1,
            columns: 1,
            array_count: 1,
            array_stride: 0,
        }),
        Some("vector") => Ok(ValueShape {
            scalar_type: parse_scalar(required_object(ty, "elementType", "vector element")?)?,
            rows: required_u32(ty, "elementCount", "vector element count")?,
            columns: 1,
            array_count: 1,
            array_stride: 0,
        }),
        Some("matrix") => Ok(ValueShape {
            scalar_type: parse_scalar(required_object(ty, "elementType", "matrix element")?)?,
            rows: required_u32(ty, "rowCount", "matrix row count")?,
            columns: required_u32(ty, "columnCount", "matrix column count")?,
            array_count: 1,
            array_stride: 0,
        }),
        Some("array") => {
            let mut element = parse_value_shape(
                required_object(ty, "elementType", "array element type")?,
                array_layout,
            )?;
            if element.array_count != 1 {
                return Err(Error::Reflection(
                    "nested shader value arrays are not supported".to_owned(),
                ));
            }
            element.array_count = required_u32(ty, "elementCount", "array element count")?;
            element.array_stride = if array_layout == ArrayLayout::Uniform {
                required_u32(ty, "uniformStride", "array uniform stride")?
            } else {
                0
            };
            Ok(element)
        }
        kind => Err(Error::Reflection(format!(
            "unsupported shader value kind {kind:?}"
        ))),
    }
}

fn parse_scalar(ty: &Value) -> Result<ShaderScalarType> {
    match ty.get("scalarType").and_then(Value::as_str) {
        Some("bool") => Ok(ShaderScalarType::Bool),
        Some("int32") => Ok(ShaderScalarType::I32),
        Some("uint32") => Ok(ShaderScalarType::U32),
        Some("float32") => Ok(ShaderScalarType::F32),
        scalar => Err(Error::Reflection(format!(
            "unsupported shader scalar type {scalar:?}"
        ))),
    }
}

fn occupied_struct_bytes(ty: &Value) -> Option<u32> {
    ty.get("fields")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|field| {
            let binding = field.get("binding")?;
            let offset = u32::try_from(binding.get("offset")?.as_u64()?).ok()?;
            let size = u32::try_from(binding.get("size")?.as_u64()?).ok()?;
            offset.checked_add(size)
        })
        .max()
}

fn binding_kind(value: &Value) -> Option<&str> {
    value
        .get("binding")
        .and_then(|binding| binding.get("kind"))
        .and_then(Value::as_str)
}

fn binding_u32(value: &Value, field: &str, label: &str) -> Result<u32> {
    let binding = value
        .get("binding")
        .ok_or_else(|| Error::Reflection(format!("{label} has no binding")))?;
    required_u32(binding, field, label)
}

fn required_string<'a>(value: &'a Value, field: &str, label: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Reflection(format!("{label} is missing")))
}

fn required_u32(value: &Value, field: &str, label: &str) -> Result<u32> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value != 0 || field == "offset" || field == "index")
        .ok_or_else(|| Error::Reflection(format!("{label} is missing or invalid")))
}

fn required_object<'a>(value: &'a Value, field: &str, label: &str) -> Result<&'a Value> {
    value
        .get(field)
        .filter(|value| value.is_object())
        .ok_or_else(|| Error::Reflection(format!("{label} is missing")))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn extracts_uniform_layout_and_struct_stage_inputs() {
        let reflection = json!({
            "parameters": [{
                "name": "globals",
                "binding": {"kind": "constantBuffer", "index": 0},
                "type": {
                    "kind": "constantBuffer",
                    "elementType": {"kind": "struct", "fields": [{
                        "name": "globals",
                        "type": {"kind": "struct", "fields": [
                            {"name": "scalarValue", "type": {"kind": "scalar", "scalarType": "float32"}, "binding": {"kind": "uniform", "offset": 0, "size": 4, "elementStride": 0}},
                            {"name": "vectorValue", "type": {"kind": "vector", "elementCount": 3, "elementType": {"kind": "scalar", "scalarType": "float32"}}, "binding": {"kind": "uniform", "offset": 4, "size": 12, "elementStride": 4}},
                            {"name": "matrixValue", "type": {"kind": "matrix", "rowCount": 4, "columnCount": 4, "elementType": {"kind": "scalar", "scalarType": "float32"}}, "binding": {"kind": "uniform", "offset": 16, "size": 64, "elementStride": 0}},
                            {"name": "arrayValue", "type": {"kind": "array", "elementCount": 3, "uniformStride": 16, "elementType": {"kind": "scalar", "scalarType": "float32"}}, "binding": {"kind": "uniform", "offset": 80, "size": 36, "elementStride": 16}}
                        ]},
                        "binding": {"kind": "uniform", "offset": 0, "size": 116, "elementStride": 0}
                    }]},
                    "elementVarLayout": {"binding": {"kind": "uniform", "offset": 0, "size": 116, "elementStride": 0}}
                }
            }],
            "entryPoints": [{
                "name": "main", "stage": "vertex",
                "parameters": [{
                    "name": "input", "binding": {"kind": "varyingInput", "index": 0},
                    "type": {"kind": "struct", "fields": [
                        {"name": "position", "binding": {"kind": "varyingInput", "index": 0}, "type": {"kind": "vector", "elementCount": 3, "elementType": {"kind": "scalar", "scalarType": "float32"}}},
                        {"name": "uv", "binding": {"kind": "varyingInput", "index": 1}, "type": {"kind": "vector", "elementCount": 2, "elementType": {"kind": "scalar", "scalarType": "float32"}}}
                    ]}
                }],
                "result": {"name": "position", "binding": {"kind": "varyingOutput", "index": 0}, "type": {"kind": "vector", "elementCount": 4, "elementType": {"kind": "scalar", "scalarType": "float32"}}}
            }]
        });

        let interface = reflect_shader_interface(&reflection, "main", ShaderStage::Vertex)
            .expect("typed interface");

        assert_eq!(interface.stage_io.len(), 3);
        assert_eq!(interface.stage_io[0].name, "position");
        assert_eq!(interface.stage_io[1].name, "uv");
        assert_eq!(interface.stage_io[2].direction, ShaderIoDirection::Output);
        let buffer = &interface.uniform_buffers[0];
        assert_eq!(
            (buffer.name.as_str(), buffer.register, buffer.byte_size),
            ("globals", 0, 116)
        );
        assert_eq!(buffer.members[0].name, "scalarValue");
        assert_eq!(buffer.members[2].matrix_stride, 16);
        assert_eq!(buffer.members[3].array_count, 3);
        assert_eq!(buffer.members[3].array_stride, 16);
    }

    #[test]
    fn rejects_duplicate_stage_locations() {
        let reflection = json!({
            "parameters": [],
            "entryPoints": [{
                "name": "main", "stage": "fragment",
                "parameters": [
                    {"name": "a", "binding": {"kind": "varyingInput", "index": 0}, "type": {"kind": "scalar", "scalarType": "float32"}},
                    {"name": "b", "binding": {"kind": "varyingInput", "index": 0}, "type": {"kind": "scalar", "scalarType": "float32"}}
                ]
            }]
        });

        assert!(reflect_shader_interface(&reflection, "main", ShaderStage::Fragment).is_err());
    }

    #[test]
    fn extracts_array_stage_io_location_span_without_uniform_stride() {
        let reflection = json!({
            "parameters": [],
            "entryPoints": [{
                "name": "main", "stage": "fragment",
                "parameters": [
                    {
                        "name": "audioValue",
                        "binding": {"kind": "varyingInput", "index": 0},
                        "type": {
                            "kind": "array", "elementCount": 16,
                            "elementType": {
                                "kind": "vector", "elementCount": 4,
                                "elementType": {"kind": "scalar", "scalarType": "float32"}
                            }
                        }
                    },
                    {
                        "name": "uv", "binding": {"kind": "varyingInput", "index": 16},
                        "type": {
                            "kind": "vector", "elementCount": 2,
                            "elementType": {"kind": "scalar", "scalarType": "float32"}
                        }
                    }
                ]
            }]
        });

        let interface = reflect_shader_interface(&reflection, "main", ShaderStage::Fragment)
            .expect("array stage interface");
        assert_eq!(interface.stage_io[0].name, "audioValue");
        assert_eq!(interface.stage_io[0].location_count, 16);
        assert_eq!(interface.stage_io[1].location, 16);
    }
}
