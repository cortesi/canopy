let t = open(["cargo", "run", "--example", "widget", "--", "font", "--exit-after-cycle"], #{width: 100, height: 25 });
t.defaults(#{ ppp: 2.0, out_ppp: 1.0 });
t.record_gif("demos/font.gif");
t.wait_child_exit(5000);
