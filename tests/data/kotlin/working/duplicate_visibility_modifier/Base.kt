object DungeonFinderFeatures {

    @HandleEvent(onlyOnSkyblock = true) fun onInventoryOpen() {
        floorStackSize = stackTip(event)
        highlightParty = highlightingHandler(event)
        toolTipMap = toolTipHandler(event)
    }
}