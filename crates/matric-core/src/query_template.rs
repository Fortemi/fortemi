//! Provider-independent query preparation and MRL postprocessing.

pub const MAX_QUERY_TEMPLATE_BYTES: usize = 2048;

pub fn validate_query_template(template: &str) -> Result<(), String> {
    if template.len() > MAX_QUERY_TEMPLATE_BYTES {
        return Err("query_template must be at most 2048 UTF-8 bytes".to_string());
    }
    if template.matches("{text}").count() != 1 {
        return Err("query_template must contain exactly one {text} placeholder".to_string());
    }
    Ok(())
}

pub fn apply_query_template(template: Option<&str>, text: &str) -> Result<String, String> {
    match template {
        Some(template) => {
            validate_query_template(template)?;
            Ok(template.replacen("{text}", text, 1))
        }
        None => Ok(text.to_string()),
    }
}

/// Validate raw provider size independently from the pgvector storage cap.
pub fn validate_provider_dimension(
    dimension: i32,
    provider_dimension: Option<i32>,
) -> Result<(), String> {
    if provider_dimension.is_some_and(|raw| raw < dimension || raw > 16384 || raw <= 0) {
        return Err("provider_dimension must be at least dimension and at most 16384".to_string());
    }
    Ok(())
}

/// Accept native or already truncated provider output, normalizing only for MRL.
pub fn postprocess_embedding(
    vector: crate::Vector,
    dimension: usize,
    raw_dimension: usize,
) -> Result<crate::Vector, String> {
    let values = vector.as_slice();
    if dimension == 0
        || raw_dimension < dimension
        || (values.len() != dimension && values.len() != raw_dimension)
    {
        return Err("Embedding dimension does not match its contract".to_string());
    }
    if dimension == raw_dimension {
        return Ok(vector);
    }
    let mut values = values[..dimension].to_vec();
    let norm = values
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    if !norm.is_finite() || norm == 0.0 {
        return Err("Truncated embedding must have a finite nonzero norm".to_string());
    }
    for value in &mut values {
        *value = (f64::from(*value) / norm) as f32;
    }
    Ok(values.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_requires_exactly_one_placeholder_and_a_bounded_byte_length() {
        for invalid in ["", "Query", "{TEXT}", "{text}{text}"] {
            assert!(validate_query_template(invalid).is_err());
        }
        let boundary = format!(
            "{}{text}",
            "a".repeat(MAX_QUERY_TEMPLATE_BYTES - 6),
            text = "{text}"
        );
        assert!(validate_query_template(&boundary).is_ok());
        assert!(validate_query_template(&format!("a{boundary}")).is_err());
        assert!(
            validate_query_template(&format!("{}{text}", "é".repeat(1022), text = "{text}"))
                .is_err()
        );
    }

    #[test]
    fn application_preserves_text_and_does_not_reapply_placeholders_in_the_query() {
        let text = "company {text}\nprofile";
        assert_eq!(apply_query_template(None, text).unwrap(), text);
        assert_eq!(
            apply_query_template(Some("Instruct: Find companies\nQuery:{text}"), text).unwrap(),
            "Instruct: Find companies\nQuery:company {text}\nprofile"
        );
        assert!(apply_query_template(Some("unusable"), text).is_err());
    }

    #[test]
    fn mrl_truncates_before_l2_normalization() {
        let vector = postprocess_embedding(vec![3.0, 4.0, 12.0].into(), 2, 3).unwrap();
        assert_eq!(vector.as_slice(), &[0.6, 0.8]);
        let norm: f32 = vector.as_slice().iter().map(|v| v * v).sum();
        assert!((norm - 1.0).abs() < 1e-6);
        assert_eq!(
            postprocess_embedding(vec![3.0, 4.0].into(), 2, 3)
                .unwrap()
                .as_slice(),
            &[0.6, 0.8]
        );
        assert_eq!(
            postprocess_embedding(vec![3.0, 4.0].into(), 2, 2)
                .unwrap()
                .as_slice(),
            &[3.0, 4.0]
        );
        assert!(postprocess_embedding(vec![0.0, 0.0, 1.0].into(), 2, 3).is_err());
        assert!(postprocess_embedding(vec![1.0].into(), 2, 3).is_err());
    }
}
