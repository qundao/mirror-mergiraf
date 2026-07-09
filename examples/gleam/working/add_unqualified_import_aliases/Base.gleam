import gleam/list.{map}

pub fn main(values) {
  map(values, fn(value) { value })
}
