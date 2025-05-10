json func() {
  return json{{"num", 0},
<<<<<<< LEFT
              {"list", json::array({{{"a", 1}, {"b", 2}}})},
||||||| BASE
              {{"num", 0},
              {"obj", {{"a", 1}, {"b", 2}}},
              {"list", json::array({{{"a", 3}, {"b", 4}}})}},
=======
              {{"num", 0},
              {"obj", {{"a", 1}, {"b", 2}}},
              {"list", json::array({{{"a", 3}, {"b", 4}}})},
              {"c", 5}},
>>>>>>> RIGHT
              {"other", {{"a", 3}, {"b", 4}}},
              {"c", 5}};
}
