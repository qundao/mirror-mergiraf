mod tests {
    pub fn foo() {}
    pub fn bar() {}

    #[cfg(feature = "tracing")]
    pub fn execute() {}
}
