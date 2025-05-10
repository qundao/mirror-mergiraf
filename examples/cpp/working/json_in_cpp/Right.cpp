json func() {
  return json{{"num", 0},
              {"obj", {{"a", 1}, {"b", 2}}},
              {"list", json::array({{{"a", 3}, {"b", 4}}})},
              {"c", 5}};
}
