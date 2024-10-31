class MyClass {
        @JsonProperty(value = "property", access = ACCESS.READ_ONLY)
        public int getValue() {
                return 23;
        }
}
