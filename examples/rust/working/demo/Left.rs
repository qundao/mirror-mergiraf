struct Lawnmower {
    id: u32,
    brand: String,
    model: String,
}

impl Lawnmower {
    fn find_home_station(&self) -> Option<&Station> {
        self.neighbouring_stations()
            .iter()
            .find(|station| station.id == "home" && !station.occupied && station.color == StationColor::Blue)
    }
}
