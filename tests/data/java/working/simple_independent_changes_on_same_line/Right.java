class Waschbear {
        void eat() {
                if (food.isAvailable() && stomach.howFull() < 0.1) {
                        consume(DEFAULT_FOOD_SOURCE);
                }
        }
}
