//! Tensors in and out of a session, and a small ONNX file writer for the
//! calibration models, so that no binary model lives in the repository.

use ort::session::{Session, SessionInputValue};
use ort::value::{DynValue, Tensor, ValueType};

pub type Inputs<'v> = Vec<(String, SessionInputValue<'v>)>;

pub fn floats(shape: &[i64], data: Vec<f32>) -> Result<DynValue, String> {
    Tensor::from_array((shape.to_vec(), data))
        .map(|t| t.into_dyn())
        .map_err(|e| e.to_string())
}

pub fn ints(shape: &[i64], data: Vec<i64>) -> Result<DynValue, String> {
    Tensor::from_array((shape.to_vec(), data))
        .map(|t| t.into_dyn())
        .map_err(|e| e.to_string())
}

pub fn flag(value: bool) -> Result<DynValue, String> {
    Tensor::from_array((vec![1_i64], vec![value]))
        .map(|t| t.into_dyn())
        .map_err(|e| e.to_string())
}

/// A vector of the output's values with its shape.
pub fn read_floats(value: &DynValue) -> Result<(Vec<i64>, &[f32]), String> {
    let (shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    Ok((shape.iter().copied().collect(), data))
}

pub fn read_ints(value: &DynValue) -> Result<&[i64], String> {
    let (_, data) = value
        .try_extract_tensor::<i64>()
        .map_err(|e| e.to_string())?;
    Ok(data)
}

pub fn input_name(session: &Session, index: usize) -> Result<String, String> {
    session
        .inputs()
        .get(index)
        .map(|i| i.name().to_string())
        .ok_or_else(|| "unexpected model".to_string())
}

pub fn output_names(session: &Session) -> Vec<String> {
    session
        .outputs()
        .iter()
        .map(|o| o.name().to_string())
        .collect()
}

/// Empty tensors for each `past_key_values` input of a text model's first step.
pub fn empty_past(session: &Session) -> Result<Vec<(String, DynValue)>, String> {
    let mut past = Vec::new();
    for input in session.inputs() {
        if !input.name().starts_with("past_key_values") {
            continue;
        }
        let ValueType::Tensor { shape, .. } = input.dtype() else {
            return Err("unexpected model".into());
        };
        let shape: Vec<i64> = shape
            .iter()
            .enumerate()
            .map(|(i, &d)| match (i, d) {
                (0, _) => 1,
                (_, d) if d < 0 => 0,
                (_, d) => d,
            })
            .collect();
        past.push((input.name().to_string(), floats(&shape, Vec::new())?));
    }
    Ok(past)
}

/// The values of the last position of a `[1, n, vocab]` output.
pub fn last_logits(value: &DynValue) -> Result<&[f32], String> {
    let (shape, data) = read_floats(value)?;
    let vocab = shape
        .last()
        .and_then(|&v| usize::try_from(v).ok())
        .filter(|&v| v > 0)
        .ok_or("unexpected model output")?;
    let start = data
        .len()
        .checked_sub(vocab)
        .ok_or("unexpected model output")?;
    data.get(start..)
        .ok_or_else(|| "unexpected model output".to_string())
}

/// The index of the largest value, leaving out `skip`.
pub fn best(logits: &[f32], skip: Option<u32>) -> Option<u32> {
    let skip = skip.and_then(|s| usize::try_from(s).ok());
    logits
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != skip)
        .max_by(|a, b| a.1.total_cmp(b.1))
        .and_then(|(i, _)| u32::try_from(i).ok())
}

/// Protobuf, as far as ONNX needs it.
#[derive(Default)]
pub struct Message(Vec<u8>);

impl Message {
    fn varint(&mut self, mut value: u64) {
        loop {
            let low = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                self.0.push(low);
                return;
            }
            self.0.push(low | 0x80);
        }
    }

    pub fn int(mut self, field: u64, value: u64) -> Self {
        self.varint(field << 3);
        self.varint(value);
        self
    }

    pub fn bytes(mut self, field: u64, value: &[u8]) -> Self {
        self.varint((field << 3) | 2);
        self.varint(value.len() as u64);
        self.0.extend_from_slice(value);
        self
    }

    pub fn text(self, field: u64, value: &str) -> Self {
        self.bytes(field, value.as_bytes())
    }

    pub fn nested(self, field: u64, value: Message) -> Self {
        self.bytes(field, &value.0)
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}
