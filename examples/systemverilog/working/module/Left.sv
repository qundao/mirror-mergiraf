module foo ();

  assign foo = bar[y] + 1;
  assign bar = 0;
  foo bar (x, y);

endmodule
