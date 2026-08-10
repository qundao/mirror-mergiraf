import gleam/list.{filter, fold, map}

pub fn main(values) {
  map(values, fn(value) { value })
}
