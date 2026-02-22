use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result, anyhow};
use ort::{session::Session, value::Tensor};
use tokenizers::{EncodeInput, Tokenizer};

use crate::config::{EmbeddingConfig, ExecutionDevice};

pub struct EmbeddingEngine {
    config: EmbeddingConfig,
    device_used: ExecutionDevice,
    backend: EmbeddingBackend,
}

enum EmbeddingBackend {
    Onnx(OnnxBackend),
    Pseudo,
    Unavailable(String),
}

struct OnnxBackend {
    session: Mutex<Session>,
    tokenizer: Option<Arc<Tokenizer>>,
}

struct EncodedBatch {
    input_ids: Vec<i64>,
    attention_mask: Vec<i64>,
    batch_size: usize,
    seq_len: usize,
}

impl EmbeddingEngine {
    pub fn new(config: EmbeddingConfig) -> Self {
        let device_used = resolve_device(config.execution_device);
        let backend = match build_backend(&config) {
            Ok(backend) => backend,
            Err(err) => EmbeddingBackend::Unavailable(err.to_string()),
        };
        Self {
            config,
            device_used,
            backend,
        }
    }

    pub fn runtime_name(&self) -> &'static str {
        "ort"
    }

    pub fn device_mode(&self) -> &'static str {
        match self.device_used {
            ExecutionDevice::Cpu => "cpu",
            ExecutionDevice::GpuPreferred => "gpu",
        }
    }

    pub fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        match &self.backend {
            EmbeddingBackend::Onnx(backend) => backend.embed_batch(inputs, &self.config),
            EmbeddingBackend::Pseudo => Ok(inputs
                .iter()
                .map(|input| pseudo_embed(input, self.config.vector_dim))
                .collect::<Vec<_>>()),
            EmbeddingBackend::Unavailable(msg) => Err(anyhow!(
                "embedding unavailable: {msg}. set CODEVIX_ALLOW_PSEUDO_EMBED=true only for local test scaffolding"
            )),
        }
    }
}

impl OnnxBackend {
    fn embed_batch(&self, inputs: &[String], cfg: &EmbeddingConfig) -> Result<Vec<Vec<f32>>> {
        let encoded = encode_inputs(inputs, cfg, self.tokenizer.as_ref())?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow!("embedding session lock poisoned"))?;

        let ids_tensor = Tensor::<i64>::from_array((
            vec![encoded.batch_size as i64, encoded.seq_len as i64],
            encoded.input_ids.clone(),
        ))?;
        let mask_tensor = Tensor::<i64>::from_array((
            vec![encoded.batch_size as i64, encoded.seq_len as i64],
            encoded.attention_mask.clone(),
        ))?;
        let token_type_tensor = Tensor::<i64>::from_array((
            vec![encoded.batch_size as i64, encoded.seq_len as i64],
            vec![0i64; encoded.batch_size * encoded.seq_len],
        ))?;

        let mut model_inputs = HashMap::new();
        for input in session.inputs() {
            let name = input.name().to_lowercase();
            let value = if name.contains("attention") && name.contains("mask") {
                mask_tensor.clone().upcast()
            } else if name.contains("token_type") {
                token_type_tensor.clone().upcast()
            } else {
                ids_tensor.clone().upcast()
            };
            model_inputs.insert(input.name().to_string(), value);
        }

        let mut outputs = session.run(model_inputs)?;
        let first_key = outputs
            .keys()
            .next()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("embedding model returned no outputs"))?;
        let output = outputs
            .remove(first_key)
            .ok_or_else(|| anyhow!("embedding model output extraction failed"))?;
        let (shape, values) = output
            .try_extract_tensor::<f32>()
            .map_err(|err| anyhow!("embedding output decode failed: {err}"))?;

        decode_output_vectors(
            shape,
            values,
            encoded.batch_size,
            encoded.seq_len,
            &encoded.attention_mask,
            cfg.vector_dim,
        )
    }
}

fn decode_output_vectors(
    shape: &[i64],
    values: &[f32],
    batch_size: usize,
    seq_len: usize,
    attention_mask: &[i64],
    target_dim: usize,
) -> Result<Vec<Vec<f32>>> {
    if shape.len() < 2 {
        return Err(anyhow!(
            "embedding output rank {} is unsupported",
            shape.len()
        ));
    }
    if shape[0] <= 0 {
        return Err(anyhow!(
            "embedding output batch dimension is invalid: {}",
            shape[0]
        ));
    }

    if shape.len() == 2 {
        let hidden = usize::try_from(shape[1]).unwrap_or(0);
        if hidden == 0 {
            return Err(anyhow!("embedding output hidden dimension is invalid"));
        }
        if values.len() < batch_size * hidden {
            return Err(anyhow!(
                "embedding output tensor too small for expected shape {}x{}",
                batch_size,
                hidden
            ));
        }

        let mut out = Vec::with_capacity(batch_size);
        for batch in 0..batch_size {
            let start = batch * hidden;
            let end = start + hidden;
            out.push(fit_vector_dim(&values[start..end], target_dim));
        }
        return Ok(out);
    }

    let hidden = usize::try_from(shape[shape.len() - 1]).unwrap_or(0);
    if hidden == 0 {
        return Err(anyhow!("embedding output hidden dimension is invalid"));
    }
    let model_seq_len = usize::try_from(shape[shape.len() - 2]).unwrap_or(seq_len);
    if values.len() < batch_size * model_seq_len * hidden {
        return Err(anyhow!(
            "embedding output tensor too small for pooled decoding"
        ));
    }

    let mut out = Vec::with_capacity(batch_size);
    for batch in 0..batch_size {
        let mut pooled = vec![0.0f32; hidden];
        let mut denom = 0.0f32;
        for token in 0..model_seq_len {
            let mask_index = batch * seq_len + token.min(seq_len.saturating_sub(1));
            if attention_mask.get(mask_index).copied().unwrap_or(0) == 0 {
                continue;
            }
            denom += 1.0;
            let base = (batch * model_seq_len + token) * hidden;
            for hidden_idx in 0..hidden {
                pooled[hidden_idx] += values[base + hidden_idx];
            }
        }
        if denom > 0.0 {
            for value in &mut pooled {
                *value /= denom;
            }
        }
        out.push(fit_vector_dim(&pooled, target_dim));
    }
    Ok(out)
}

fn fit_vector_dim(values: &[f32], target_dim: usize) -> Vec<f32> {
    if target_dim == 0 {
        return Vec::new();
    }
    if values.len() == target_dim {
        return values.to_vec();
    }
    if values.len() > target_dim {
        return values[..target_dim].to_vec();
    }
    let mut out = vec![0.0f32; target_dim];
    out[..values.len()].copy_from_slice(values);
    out
}

fn build_backend(config: &EmbeddingConfig) -> Result<EmbeddingBackend> {
    if config.allow_pseudo_fallback {
        return Ok(EmbeddingBackend::Pseudo);
    }
    let model_path = Path::new(&config.model_path);
    if !model_path.exists() {
        return Err(anyhow!(
            "embedding model not found at {}",
            model_path.display()
        ));
    }

    let session = Session::builder()
        .context("failed to create ONNX session builder")?
        .commit_from_file(model_path)
        .with_context(|| format!("failed to load ONNX model {}", model_path.display()))?;
    let tokenizer = load_tokenizer(config)?;

    Ok(EmbeddingBackend::Onnx(OnnxBackend {
        session: Mutex::new(session),
        tokenizer,
    }))
}

fn load_tokenizer(config: &EmbeddingConfig) -> Result<Option<Arc<Tokenizer>>> {
    let Some(path) = config.tokenizer_path.as_ref() else {
        return Ok(None);
    };
    let tokenizer = Tokenizer::from_file(path)
        .map_err(|err| anyhow!("failed loading tokenizer from {}: {err}", path))?;
    Ok(Some(Arc::new(tokenizer)))
}

fn encode_inputs(
    inputs: &[String],
    config: &EmbeddingConfig,
    tokenizer: Option<&Arc<Tokenizer>>,
) -> Result<EncodedBatch> {
    if let Some(tokenizer) = tokenizer {
        let encoded_inputs = inputs
            .iter()
            .map(|text| EncodeInput::Single(text.as_str().into()))
            .collect::<Vec<_>>();
        let encodings = tokenizer
            .encode_batch(encoded_inputs, true)
            .map_err(|err| anyhow!("tokenization failed: {err}"))?;

        let seq_len = config.max_sequence_length.max(1);
        let mut input_ids = vec![0i64; inputs.len() * seq_len];
        let mut attention_mask = vec![0i64; inputs.len() * seq_len];
        for (row, encoding) in encodings.iter().enumerate() {
            for (col, token_id) in encoding.get_ids().iter().take(seq_len).enumerate() {
                input_ids[row * seq_len + col] = i64::from(*token_id);
                attention_mask[row * seq_len + col] = 1;
            }
        }

        return Ok(EncodedBatch {
            input_ids,
            attention_mask,
            batch_size: inputs.len(),
            seq_len,
        });
    }

    let seq_len = config.max_sequence_length.max(1);
    let mut input_ids = vec![0i64; inputs.len() * seq_len];
    let mut attention_mask = vec![0i64; inputs.len() * seq_len];
    for (row, text) in inputs.iter().enumerate() {
        for (col, byte) in text.as_bytes().iter().take(seq_len).enumerate() {
            input_ids[row * seq_len + col] = i64::from(*byte) + 1;
            attention_mask[row * seq_len + col] = 1;
        }
    }
    Ok(EncodedBatch {
        input_ids,
        attention_mask,
        batch_size: inputs.len(),
        seq_len,
    })
}

fn resolve_device(preferred: ExecutionDevice) -> ExecutionDevice {
    match preferred {
        ExecutionDevice::Cpu => ExecutionDevice::Cpu,
        ExecutionDevice::GpuPreferred => {
            if gpu_runtime_available() {
                ExecutionDevice::GpuPreferred
            } else {
                ExecutionDevice::Cpu
            }
        }
    }
}

fn gpu_runtime_available() -> bool {
    std::env::var("EMBEDDING_GPU_AVAILABLE")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn pseudo_embed(input: &str, dim: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; dim.max(1)];
    let n = out.len();
    for (idx, b) in input.as_bytes().iter().enumerate() {
        out[idx % n] += (*b as f32) / 255.0;
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::{EmbeddingConfig, EmbeddingEngine, ExecutionDevice};

    #[test]
    fn embeds_batch_with_expected_dimensions_in_pseudo_mode() {
        let engine = EmbeddingEngine::new(EmbeddingConfig {
            vector_dim: 8,
            ..EmbeddingConfig::default()
        });
        let vectors = engine
            .embed_batch(&["hello".to_string(), "world".to_string()])
            .expect("pseudo vectors");
        assert_eq!(vectors.len(), 2);
        assert_eq!(vectors[0].len(), 8);
        assert_eq!(engine.runtime_name(), "ort");
    }

    #[test]
    fn gpu_preferred_falls_back_to_cpu_when_unavailable() {
        let engine = EmbeddingEngine::new(EmbeddingConfig {
            execution_device: ExecutionDevice::GpuPreferred,
            ..EmbeddingConfig::default()
        });
        assert_eq!(engine.device_mode(), "cpu");
    }

    #[test]
    fn reports_model_error_when_pseudo_disabled() {
        let engine = EmbeddingEngine::new(EmbeddingConfig {
            model_path: "/tmp/does-not-exist.onnx".to_string(),
            allow_pseudo_fallback: false,
            ..EmbeddingConfig::default()
        });
        let err = engine
            .embed_batch(&["hello".to_string()])
            .expect_err("missing model should be reported");
        assert!(err.to_string().contains("embedding unavailable"));
    }
}
