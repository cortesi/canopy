//! Compile-time contracts of the option derives.

#[cfg(test)]
mod tests {
    #[test]
    fn skip_and_flatten_attributes() {
        let cases = trybuild::TestCases::new();
        cases.pass("tests/ui/skip_default.rs");
        cases.compile_fail("tests/ui/skip_with_key.rs");
        cases.compile_fail("tests/ui/skip_and_flatten.rs");
    }
}
