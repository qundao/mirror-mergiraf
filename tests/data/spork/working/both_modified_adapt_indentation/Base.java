class MyClass {
    boolean reallyEnabled = true;
    int myMethod() {
        boolean enabled = false;
        if (enabled) {
            if (reallyEnabled) {
                doSomething("param");
                getBusy(123);
            }
        }
        checkThisOut("badoum", "tss");
    }
}
