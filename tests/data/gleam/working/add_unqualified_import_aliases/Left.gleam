import gleam/list.{filter as keep, map}

pub fn main(values) {
  map(values, fn(value) { value })
}
