use crate::provider::Track;
use rand::RngExt;
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

pub fn normalize(text: &str) -> String {
    text.nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .nfkc()
        .collect()
}

fn graphemes(text: &str) -> Vec<String> {
    let s = normalize(text);
    s.graphemes(true).take(256).map(str::to_owned).collect()
}

// Bounded O(256²) dynamic programming: edit distance, common substring and LCS.
pub fn similarity(query: &str, text: &str) -> f64 {
    let a = graphemes(query);
    let b = graphemes(text);
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let mut edit: Vec<usize> = (0..=b.len()).collect();
    let mut lcs = vec![0; b.len() + 1];
    let mut run = vec![0; b.len() + 1];
    let mut longest = 0;
    for (i, x) in a.iter().enumerate() {
        let (mut prev_edit, mut prev_lcs, mut prev_run) = (edit[0], 0, 0);
        edit[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let k = j + 1;
            let (e, l, r) = (edit[k], lcs[k], run[k]);
            edit[k] = (prev_edit + usize::from(x != y))
                .min(edit[k - 1] + 1)
                .min(e + 1);
            lcs[k] = if x == y {
                prev_lcs + 1
            } else {
                lcs[k - 1].max(l)
            };
            run[k] = if x == y { prev_run + 1 } else { 0 };
            longest = longest.max(run[k]);
            (prev_edit, prev_lcs, prev_run) = (e, l, r);
        }
    }
    let aa: HashSet<_> = a.iter().filter(|s| !s.trim().is_empty()).collect();
    let bb: HashSet<_> = b.iter().filter(|s| !s.trim().is_empty()).collect();
    let overlap = aa.intersection(&bb).count() as f64 / aa.union(&bb).count().max(1) as f64;
    let max_len = a.len().max(b.len()) as f64;
    let edit_score = 1.0 - edit[b.len()] as f64 / max_len;
    let subseq = 2.0 * lcs[b.len()] as f64 / (a.len() + b.len()) as f64;
    let contiguous = 2.0 * longest as f64 / (a.len() + b.len()) as f64;
    // A short accidental common character must not qualify as a match.
    if a.len() > 2 && longest < 2 && edit_score < 0.65 {
        return 0.0;
    }
    (0.30 * edit_score + 0.20 * overlap + 0.20 * subseq + 0.30 * contiguous).clamp(0.0, 1.0)
}

pub fn score(query: &str, track: &Track) -> f64 {
    similarity(query, &track.name)
        .max(0.85 * similarity(query, &track.artist_name))
        .max(0.70 * similarity(query, &track.album_name))
}

pub fn choose<'a>(query: &str, tracks: &'a [Track], rng: &mut impl rand::Rng) -> Option<&'a Track> {
    let mut seen = HashSet::new();
    let scored: Vec<_> = tracks
        .iter()
        .filter(|t| t.eligible() && seen.insert(&t.id))
        .map(|t| (t, score(query, t)))
        .filter(|(_, s)| *s >= 0.48)
        .collect();
    let best = scored.iter().map(|(_, s)| *s).fold(0.0, f64::max);
    let candidates: Vec<_> = scored
        .into_iter()
        .filter(|(_, s)| *s >= best - 0.22)
        .collect();
    let sum: f64 = candidates
        .iter()
        .map(|(_, s)| ((s - best) / 0.14).exp())
        .sum();
    if candidates.is_empty() {
        return None;
    }
    let mut ticket = rng.random::<f64>() * sum;
    for (track, s) in &candidates {
        ticket -= ((s - best) / 0.14).exp();
        if ticket <= 0.0 {
            return Some(track);
        }
    }
    candidates.last().map(|(t, _)| *t)
}

pub fn fragment(text: &str, rng: &mut impl rand::Rng) -> Option<String> {
    let gs: Vec<_> = text.graphemes(true).collect();
    if gs.len() < 4 {
        return None;
    }
    let length = rng.random_range(2..=gs.len().min(12).min(gs.len() - 1));
    let start = rng.random_range(0..=gs.len() - length);
    let result = gs[start..start + length].concat();
    if result.trim().graphemes(true).count() < 2 || result == text {
        None
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};
    #[test]
    fn unicode_ranking() {
        assert_eq!(normalize("Ｃafé E\u{301}"), "café é");
        assert_eq!(similarity("CAFÉ", "cafe\u{301}"), 1.0);
        assert!(
            similarity("Clair de lune", "Clair de lune")
                > similarity("Clair de lune", "Clair de lune live")
        );
        assert!(
            similarity("Clair de lune", "Clair de lune live")
                > similarity("Clair de lune", "Death metal")
        );
        assert_eq!(similarity("明天晴天", "黑夜"), 0.0);
        assert!(similarity("abcdef", "axbxcxdxexf") < 0.48);
    }
    #[test]
    fn fragments_are_graphemes_and_bounded() {
        let mut rng = StdRng::seed_from_u64(42);
        let s = "👩‍💻e\u{301}🇫🇷日文";
        let boundaries: HashSet<_> = s
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([s.len()])
            .collect();
        for _ in 0..100 {
            let f = fragment(s, &mut rng).unwrap();
            let pos = s.find(&f).unwrap();
            assert!(boundaries.contains(&pos) && boundaries.contains(&(pos + f.len())));
        }
        assert!(fragment("a中", &mut rng).is_none());
    }
    #[test]
    fn seeded_weighted_selection_filters_and_varies() {
        let tracks = vec![
            Track::fixture("1", "Clair de lune"),
            Track::fixture("2", "Clair de lune"),
            Track::fixture("3", "ZZZZZZ"),
        ];
        let mut a = StdRng::seed_from_u64(22);
        let mut b = StdRng::seed_from_u64(22);
        let mut ids = HashSet::new();
        for _ in 0..100 {
            let x = choose("Clair de lune", &tracks, &mut a).unwrap();
            assert_eq!(x.id, choose("Clair de lune", &tracks, &mut b).unwrap().id);
            assert_ne!(x.id, "3");
            ids.insert(&x.id);
        }
        assert_eq!(ids.len(), 2);
        assert!(choose("none", &[], &mut a).is_none());
    }
}
