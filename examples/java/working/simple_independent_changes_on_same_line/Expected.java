class Waschbear {

        Environment env;

        void eat() {
                if (food.isAvailable(env) && stomach.howFull() < 0.1) {
                        consume(DEFAULT_FOOD_SOURCE);
                }
        }
}
