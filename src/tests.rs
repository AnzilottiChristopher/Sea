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

    #[test]
    fn test_array_alias_free_no_issues() {
        // allocation bound to a local, then moved into an array slot via a
        // plain identifier assignment (`args[i] = arg;`), freed later through
        // the array — the local must not be reported as leaked.
        let messages = run("examples/array_alias_free.c");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }

    // Sea specific tests
    #[test]
    fn test_sea_malloc_no_drop() {
        let messages = run("examples/sea/malloc_no_drop.sea");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("uses malloc in constructor but has no drop")),
            "expected malloc without drop error but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_sea_malloc_with_drop() {
        let messages = run("examples/sea/malloc_with_drop.sea");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }
    #[test]
    fn test_sea_no_constructor() {
        let messages = run("examples/sea/no_constructor.sea");
        assert!(
            messages.iter().any(|m| m.contains("has no constructor")),
            "expected no constructor error but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_sea_multiple_constructors() {
        let messages = run("examples/sea/multiple_constructors.sea");
        assert!(
            messages.iter().any(|m| m.contains("multiple constructors")),
            "expected multiple constructors error but got: {:?}",
            messages
        );
    }
    #[test]
    fn test_sea_missing_interface_method() {
        let messages = run("examples/sea/missing_interface_method.sea");
        assert!(
            messages.iter().any(|m| m.contains("missing method 'swim'")),
            "expected missing method error but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_sea_valid_interface() {
        let messages = run("examples/sea/valid_interface.sea");
        assert!(
            messages.is_empty(),
            "expected no issues but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_sea_unknown_interface() {
        let messages = run("examples/sea/unknown_interface.sea");
        assert!(
            messages.iter().any(|m| m.contains("unknown interface")),
            "expected unknown interface error but got: {:?}",
            messages
        );
    }
    #[test]
    fn test_sea_drop_no_free() {
        let messages = run("examples/sea/drop_no_free.sea");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("drop() but never calls free()")),
            "expected drop without free warning but got: {:?}",
            messages
        );
    }
    #[test]
    fn test_sea_unknown_parent() {
        let messages = run("examples/sea/unknown_parent.sea");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("inherits unknown class")),
            "expected unknown parent error but got: {:?}",
            messages
        );
    }
    #[test]
    fn test_sea_c_style_method() {
        let messages = run("examples/sea/c_style_method.sea");
        assert!(
            messages.iter().any(|m| m.contains("C style method")),
            "expected C style method warning but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_param_deref_no_issues() {
        let messages = run("examples/param_deref.c");
        assert!(
            messages.is_empty(),
            "function parameters are initialized by the caller and shouldn't be \
             flagged as uninitialized, regardless of pointer depth or whether they're \
             dereferenced via `*p` or `p[i]`, but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_cast_init_no_issues() {
        let messages = run("examples/cast_init_no_issues.c");
        assert!(
            !messages.iter().any(|m| m.contains("'targ'")),
            "a pointer declared and initialized in one statement via a cast \
             (`T *targ = (T *)args;`) must not be treated as uninitialized, \
             but got: {:?}",
            messages
        );
    }

    #[test]
    fn test_multi_function_use_before_init() {
        let messages = run("examples/multi_function_use_before_init.c");
        assert!(
            messages
                .iter()
                .any(|m| m.contains("use of uninitialized pointer 'p'")),
            "expected a genuine uninitialized-local diagnostic for 'p' but got: {:?}",
            messages
        );
        assert!(
            !messages.iter().any(|m| m.contains("'s'")),
            "parameter 's' in an earlier function must not leak a diagnostic into \
             or out of the next function's analysis, but got: {:?}",
            messages
        );
    }
}
