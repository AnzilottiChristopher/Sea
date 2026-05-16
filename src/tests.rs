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
}
