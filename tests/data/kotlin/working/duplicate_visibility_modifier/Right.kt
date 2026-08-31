object DungeonFinderFeatures {

    @HandleEvent(onlyOnSkyblock = true)
    private fun onInventoryOpen() {
        floorStackSize = stackTip(event)
        highlightParty = highlightingHandler(event)
        toolTipMap = toolTipHandler(event)
    }
}