import gleam/list.{filter, map}

pub fn main(values) {
  map(values, fn(value) { value })
}
