class Main {
        void runNow() {
                System.out.println("it works!");
        }

        int getSatisfaction() {
                throw new IllegalStateException("I can't get no satisfaction!");
        }

        void runNow(Environment env) {
                System.out.println("it also works!");
        }
}
