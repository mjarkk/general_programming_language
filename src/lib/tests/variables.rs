use super::*;

#[test]
fn test_variable() {
  parse_str(
    r#"
                const foo = "1234"
            "#,
  );
}

#[test]
fn test_variable_string_with_spaces() {
  parse_str(
    r#"
                const foo = "Hello world!"
            "#,
  );
}

#[test]
fn test_variable_strings_with_backslashes() {
  parse_str(
    r#"
                const foo = "I like to say \"Hello World!\" in my code."
                const bar = "This \\ backslash is displayed when printed!"
            "#,
  );
}

#[test]
fn test_variable_global_let() {
  parse_str_fail(
    r#"
                let foo = 0
            "#,
  );
}
