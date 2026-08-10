import gleam/list.{fold as reduce, map}

pub fn main(values) {
  map(values, fn(value) { value })
}
