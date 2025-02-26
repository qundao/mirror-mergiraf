module foo ();

  assign foo = bar[x] + 2;
  assign bar = 5;
  foo bar (x, 1);

endmodule
