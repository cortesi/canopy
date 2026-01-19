term::defaults(#{ ppp: 2.0, out_ppp: 1.0 });

let t = open(
    ["cargo", "run", "--example", "widget", "--", "font", "--exit-after-cycle"],
    #{width: 100, height: 25 }
);
t.wait_text_absent("Running");
t.sleep(100);

t.record_gif("demos/font.gif");
t.wait_child_exit(8000);
