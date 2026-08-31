object Recipe {
<<<<<<< LEFT
    override final fun demonstrate() {
||||||| BASE
    override  fun demonstrate() {
=======
    override protected fun demonstrate() {
>>>>>>> RIGHT
        LOGGER.info { "Demo time!" }
    }
}
