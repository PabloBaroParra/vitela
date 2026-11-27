//! Whether the traces are on: the reading of the `VITELA_PERF` value.

/// `true` for the values that switch the traces on — `1`, `true`, `on`,
/// `yes`, in any case and with surrounding whitespace ignored. Unset, empty
/// or anything else leaves them off, so a typo never starts logging.
pub(super) fn parse(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        let value = value.trim();
        ["1", "true", "on", "yes"]
            .iter()
            .any(|accepted| value.eq_ignore_ascii_case(accepted))
    })
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn an_unset_variable_leaves_the_traces_off() {
        assert!(!parse(None));
    }

    #[test]
    fn the_documented_values_switch_the_traces_on() {
        for value in ["1", "true", "TRUE", "on", "Yes", " 1 "] {
            assert!(parse(Some(value)), "{value:?} should enable the traces");
        }
    }

    #[test]
    fn anything_else_leaves_the_traces_off() {
        for value in ["", "0", "false", "off", "no", "2", "enable"] {
            assert!(
                !parse(Some(value)),
                "{value:?} should not enable the traces"
            );
        }
    }
}
