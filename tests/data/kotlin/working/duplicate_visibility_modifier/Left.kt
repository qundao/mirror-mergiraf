object DungeonFinderFeatures {

    @HandleEvent(onlyOnSkyblock = true)
    private fun onInventoryOpen() {
        val inventoryName = event.inventoryName
        val inventoryItems = event.inventoryItems
        floorStackSize = stackTip(inventoryName, inventoryItems)
        highlightParty = highlightingHandler(inventoryName, inventoryItems)
        toolTipMap = toolTipHandler(inventoryName, inventoryItems)
    }
}