term::defaults(#{ ppp: 2.0, out_ppp: 2.0 });

let t = open(
    ["cargo", "run", "--release", "--example", "widget", "--", "editor"],
    #{width: 90, height: 30 }
);
t.wait_text_absent("Compiling", #{ timeout_ms: 20000 });
t.sleep(5000);


t.record_gif("demos/editor.gif", #{ fps: 10 });
t.type("jjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj", #{ cps: 12 });
t.type("ihello there!", #{ cps: 6 });
t.key("esc");
t.type("kkkk", #{ cps: 12 });

