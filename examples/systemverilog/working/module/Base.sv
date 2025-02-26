module foo ();

  assign foo = bar[x] + 1;
  assign bar = 0;
  foo bar (x, y);

endmodule
