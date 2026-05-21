#[cfg(test)]
mod tests {
    use crate::sea::Sea;
    use std::path::PathBuf;

    fn run(file: &str) -> Vec<String> {
        let path = PathBuf::from(file);
        let sea = Sea::new(&path);
        let diagnostics = sea.analyze(file);
        diagnostics.iter().map(|d| d.message.clone()).collect()
    }

    #[test]
    fn test_double_free() {
        let messages = run("examples/double_free.c");
        assert!(messages.iter().any(|m| m.contains("double free of 'p'")));
    }

    #[test]
    fn test_use_before_init() {
        let messages = run("examples/use_before_init.c");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("use of uninitialized pointer 'p'")),
            "expected use of uninitialized pointer but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_use_after_free() {
        let messages = run("examples/use_after_free.c");
        assert!(messages.iter().any(|m| m.contains("use after free of 'p'")));
    }

    #[test]
    fn test_valid_no_issues() {
        let messages = run("examples/valid.c");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_reinit_no_issues() {
        let messages = run("examples/reinit.c");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_null_deref() {
        let messages = run("examples/null_dereference.c");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("null pointer dereference"))
        );
    }

    #[test]
    fn test_stack_return() {
        let messages = run("examples/stack_return.c");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("returning address of stack variable"))
        );
    }

    #[test]
    fn test_valid_return_no_issues() {
        let messages = run("examples/valid_return.c");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_ptr_outlive() {
        let messages = run("examples/ptr_outlive.c");
        assert!(messages.iter().any(|m| m.contains("will outlive")));
    }

    #[test]
    fn test_valid_scope_no_issues() {
        let messages = run("examples/valid_scope.c");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_pass_freed() {
        let messages = run("examples/pass_freed.c");
        assert!(messages.iter().any(|m| m.contains("passing freed pointer")));
    }

    #[test]
    fn test_mixed() {
        let messages = run("examples/mixed.c");

        assert!(
            messages.iter().any(|m| m.contains("double free of 'p'")),
            "expected double free of p but got: {:?}",
            messages
        );
        assert!(
            messages.iter().any(|m| m.contains("use after free of 'q'")),
            "expected use after free of q but got: {:?}",
            messages
        );
        // r should not appear in any error
        assert!(
            !messages.iter().any(|m| m.contains("'r'")),
            "unexpected error involving r: {:?}",
            messages
        );
    }
    #[test]
    fn test_conditional_free() {
        let messages = run("examples/conditional_free.c");
        // should warn not error — maybe freed
        assert!(
            messages
                .iter()
                .any(|m| m.contains("possible use after free")),
            "expected possible use after free but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_conditional_free_both() {
        let messages = run("examples/conditional_free_both.c");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }
    #[test]
    fn test_no_else() {
        let messages = run("examples/no_else.c");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("possible double free") || m.contains("double free")),
            "expected double free warning but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_nested_if() {
        let messages = run("examples/nested_if.c");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("possible use after free")),
            "expected possible use after free but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_free_in_else() {
        let messages = run("examples/free_in_else.c");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("possible use after free")),
            "expected possible use after free but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_valid_if_else() {
        let messages = run("examples/valid_if_else.c");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }
    #[test]
    fn test_while_free() {
        let messages = run("examples/while_free.c");
        assert!(
            messages.iter().any(
                |m| m.contains("possible use after free") || m.contains("possible double free")
            ),
            "expected possible use after free or double free but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_while_valid() {
        let messages = run("examples/while_valid.c");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }
    #[test]
    fn test_for_valid() {
        let messages = run("examples/for_valid.c");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_for_free() {
        let messages = run("examples/for_free.c");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("possible use after free")),
            "expected possible use after free but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_for_double_free() {
        let messages = run("examples/for_double_free.c");
        assert!(
            messages.iter().any(|m| m.contains("possible double free")),
            "expected possible double free but got: {:?}",
            messages
        );
    }
    #[test]
    fn test_switch_valid() {
        let messages = run("examples/switch_valid.c");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_switch_free() {
        let messages = run("examples/switch_free.c");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("possible use after free")),
            "expected possible use after free but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_switch_fallthrough() {
        let messages = run("examples/switch_fallthrough.c");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("possible use after free")),
            "expected possible use after free but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_switch_all_cases() {
        let messages = run("examples/switch_all_cases.c");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }
    #[test]
    fn test_do_while_valid() {
        let messages = run("examples/do_while_valid.c");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_do_while_free() {
        let messages = run("examples/do_while_free.c");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("possible use after free")),
            "expected possible use after free but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_do_while_double_free() {
        let messages = run("examples/do_while_double_free.c");
        assert!(
            messages.iter().any(|m| m.contains("possible double free")),
            "expected possible double free but got: {:?}",
            messages
        );
    }
    #[test]
    fn test_memory_leak() {
        let messages = run("examples/leak.c");
        assert!(
            messages.iter().any(|m| m.contains("memory leak")),
            "expected memory leak but got: {:?}",
            messages
        );
    }
}
