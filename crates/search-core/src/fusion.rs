use ahash::AHashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct ScoredId {
    pub id: String,
    pub score: f32,
}

pub fn rrf_fuse(
    lexical_ids: &[String],
    vector_ids: &[String],
    k: usize,
    w_lex: f32,
    w_vec: f32,
) -> Vec<ScoredId> {
    let kf = k as f32;
    let mut scores: AHashMap<String, f32> = AHashMap::new();

    for (rank, id) in lexical_ids.iter().enumerate() {
        let rr = w_lex / (kf + (rank + 1) as f32);
        *scores.entry(id.clone()).or_insert(0.0) += rr;
    }
    for (rank, id) in vector_ids.iter().enumerate() {
        let rr = w_vec / (kf + (rank + 1) as f32);
        *scores.entry(id.clone()).or_insert(0.0) += rr;
    }

    let mut fused = scores
        .into_iter()
        .map(|(id, score)| ScoredId { id, score })
        .collect::<Vec<_>>();
    fused.sort_by(|a, b| b.score.total_cmp(&a.score));
    fused
}

#[cfg(test)]
mod tests {
    use super::rrf_fuse;

    #[test]
    fn rrf_boosts_items_present_in_both_lists() {
        let lex = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let vecs = vec!["b".to_string(), "x".to_string(), "a".to_string()];
        let fused = rrf_fuse(&lex, &vecs, 60, 1.0, 0.7);
        assert_eq!(fused[0].id, "b");
    }
}
