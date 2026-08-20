pub fn label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::label;

    #[test]
    fn escapes_prometheus_label_control_characters_in_order() {
        assert_eq!(
            label("web\\blue\"}\npam_injected 1"),
            "web\\\\blue\\\"}\\npam_injected 1"
        );
    }
}
