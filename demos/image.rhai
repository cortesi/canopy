term::defaults(#{ ppp: 2.0, out_ppp: 0.5 });


let t = open(["cargo", "run", "--release", "--example", "widget", "--", "image"], #{width: 90, height: 30 });
t.wait_text_absent("Compiling", #{ timeout_ms: 20000 });
t.sleep(100);
t.record_gif("demos/image.gif", #{ fps: 20 });

t.sleep(1000);
t.type("iii", #{ cps: 3 });
t.type("llllllljjjjjjjjkkkkk", #{ cps: 8 });
t.type("oooo", #{ cps: 3 });

t.sleep(1000);
