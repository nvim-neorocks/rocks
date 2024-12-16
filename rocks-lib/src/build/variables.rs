pub trait HasVariables {
    /// Substitute variables of the format `$(VAR)`, where `VAR` is the variable name.
    fn substitute_variables(&self, input: &str) -> String;
}

/// Substitute variables of the format `$(VAR)`, where `VAR` is the variable name
/// passed to `get_var`.
pub fn substitute<F>(get_var: F, input: &str) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let mut result = String::new();
    let mut chars = itertools::peek_nth(input.chars());
    while let Some(c) = chars.next() {
        if c == '$' {
            if let Some('(') = chars.peek() {
                chars.next();
                let mut var_name = String::new();
                while let Some(&next_char) = chars.peek() {
                    if next_char == ')' {
                        chars.next();
                        break;
                    }
                    var_name.push(next_char);
                    chars.next();
                }
                if let Some(path) = get_var(var_name.as_str()) {
                    result.push_str(&path);
                } else {
                    result.push_str(format!("$({})", var_name).as_str());
                }
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_helper() {
        assert_eq!(substitute(get_var, "$(TEST_VAR)".into()), "foo".to_string());
        assert_eq!(
            substitute(get_var, "$(UNRECOGNISED)".into()),
            "$(UNRECOGNISED)".to_string()
        );
    }

    fn get_var(var_name: &str) -> Option<String> {
        match var_name {
            "TEST_VAR" => Some("foo".into()),
            _ => None,
        }
    }
}
