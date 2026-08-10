class MyClass {
        void myMethod() {
                try {
                        doSomething();
                } catch (IOException | IllegalArgumentException e) {
                        System.out.println("oops\n");
                }
        }
}
