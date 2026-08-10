class Waschbear {
        void eat() {
                if (food.isAvailable() && stomach.isEmpty()) {
                        consume(DEFAULT_FOOD_SOURCE);
                }
        }
}
