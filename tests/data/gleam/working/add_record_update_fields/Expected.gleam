pub type User {
  User(name: String, age: Int, active: Bool)
}

pub fn update(user) {
  User(..user, name: "Ada", age: 42, active: True)
}
