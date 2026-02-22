#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalDefaults {
    pub lexical_top_k: usize,
    pub fused_top_n: usize,
}

impl Default for RetrievalDefaults {
    fn default() -> Self {
        Self {
            lexical_top_k: 20,
            fused_top_n: 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RetrievalDefaults;

    #[test]
    fn aligns_with_idea_baseline() {
        let d = RetrievalDefaults::default();
        assert_eq!(d.lexical_top_k, 20);
        assert_eq!(d.fused_top_n, 5);
    }
}
