import gleam/list.{fold, map}

pub fn main(values) {
  map(values, fn(value) { value })
}
