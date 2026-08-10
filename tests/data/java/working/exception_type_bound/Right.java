class MyClass {
        void myMethod() {
                try {
                        doSomething();
                } catch (IOException | IllegalArgumentException | NullPointerException e) {
                        System.out.println("oops\n");
                }
        }
}
