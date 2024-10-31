
impl Lawnmower {
    fn find_home_station(&self) -> Option<&Station> {
        self.neighbouring_stations()
            .iter()
            .find(|station| self.is_suitable_home(station))
    }

    fn is_suitable_home(&self, station: &Station) -> bool {
        station.id == self.home_station_id && !station.occupied && station.color == StationColor::Blue
    }
}
