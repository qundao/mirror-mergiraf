class Waschbear {

        Environment env;

        void eat() {
                if (food.isAvailable(env) && stomach.isEmpty()) {
                        consume(DEFAULT_FOOD_SOURCE);
                }
        }
}
