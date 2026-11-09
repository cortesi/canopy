//! Compile-time contracts for generated command calls and eligibility hooks.

#[cfg(test)]
mod tests {
    #[test]
    fn command_call_and_status_types() {
        let cases = trybuild::TestCases::new();
        cases.compile_fail("tests/ui/typed_call_wrong_type.rs");
        cases.compile_fail("tests/ui/status_mutable_receiver.rs");
        cases.compile_fail("tests/ui/status_wrong_return.rs");
        cases.compile_fail("tests/ui/status_wrong_params.rs");
        cases.compile_fail("tests/ui/bare_command.rs");
        cases.compile_fail("tests/ui/inject_sealed.rs");
        cases.compile_fail("tests/ui/private_command_builders.rs");
        cases.compile_fail("tests/ui/derive_commands_arguments.rs");
        cases.compile_fail("tests/ui/two_command_attributes.rs");
        cases.pass("tests/ui/shadowed_canopy.rs");
    }
}
