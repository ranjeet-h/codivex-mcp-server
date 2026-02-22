#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuantizationMode {
    None,
    Int8,
    UInt8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionDevice {
    Cpu,
    GpuPreferred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingConfig {
    pub model_path: String,
    pub tokenizer_path: Option<String>,
    pub vector_dim: usize,
    pub max_sequence_length: usize,
    pub batch_size: usize,
    pub quantization: QuantizationMode,
    pub execution_device: ExecutionDevice,
    pub allow_pseudo_fallback: bool,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        let model_path = std::env::var("CODEVIX_MODEL_PATH")
            .unwrap_or_else(|_| "models/all-minilm-l6-v2.onnx".to_string());
        let tokenizer_path = std::env::var("CODEVIX_TOKENIZER_PATH").ok().or_else(|| {
            let candidate = std::path::Path::new(&model_path).with_extension("tokenizer.json");
            if candidate.exists() {
                Some(candidate.display().to_string())
            } else {
                None
            }
        });
        Self {
            model_path,
            tokenizer_path,
            vector_dim: 384,
            max_sequence_length: 256,
            batch_size: 128,
            quantization: QuantizationMode::None,
            execution_device: ExecutionDevice::from_env(),
            allow_pseudo_fallback: std::env::var("CODEVIX_ALLOW_PSEUDO_EMBED")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(cfg!(test)),
        }
    }
}

impl ExecutionDevice {
    pub fn from_env() -> Self {
        let raw = std::env::var("EMBEDDING_DEVICE").unwrap_or_default();
        if raw.eq_ignore_ascii_case("gpu") {
            Self::GpuPreferred
        } else {
            Self::Cpu
        }
    }
}
