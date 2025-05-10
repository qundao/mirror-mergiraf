json func() {
  return json{{"num", 0},
              {"list", json::array({{{"a", 1}, {"b", 2}}})},
              {"other", {{"a", 3}, {"b", 4}}},
              {"c", 5}};
}
