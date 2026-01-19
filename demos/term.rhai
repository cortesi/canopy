term::defaults(#{ ppp: 2.0, out_ppp: 2.0 });

let t = open(
    ["cargo", "run", "--release", "--example", "widget", "--", "term"],
    #{width: 90, height: 30 }
);
t.wait_text_absent("Compiling", #{ timeout_ms: 20000 });
t.sleep(5000);

t.record_gif("demos/term.gif", #{ fps: 10 });
t.sleep(1000);
t.key("ctrl+tab");
t.sleep(1000);
t.key("ctrl+tab");
t.sleep(1000);
t.key("ctrl+tab");
t.sleep(1000);
