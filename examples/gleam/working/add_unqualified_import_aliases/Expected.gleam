import gleam/list.{filter as keep, fold as reduce, map}

pub fn main(values) {
  map(values, fn(value) { value })
}
