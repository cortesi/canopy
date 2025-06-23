// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item expanded "><a href="overview.html"><strong aria-hidden="true">1.</strong> Overview</a></li><li class="chapter-item expanded "><a href="nodes.html"><strong aria-hidden="true">2.</strong> Nodes</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="state.html"><strong aria-hidden="true">2.1.</strong> trait StatefulNode</a></li><li class="chapter-item expanded "><a href="commands.html"><strong aria-hidden="true">2.2.</strong> trait CommandNode</a></li><li class="chapter-item expanded "><a href="nodetrait.html"><strong aria-hidden="true">2.3.</strong> trait Node</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="layout.html"><strong aria-hidden="true">2.3.1.</strong> Layout and Rendering</a></li><li class="chapter-item expanded "><a href="polling.html"><strong aria-hidden="true">2.3.2.</strong> Polling</a></li><li class="chapter-item expanded "><a href="cursor.html"><strong aria-hidden="true">2.3.3.</strong> Cursor</a></li><li class="chapter-item expanded "><a href="keyboard.html"><strong aria-hidden="true">2.3.4.</strong> Key events</a></li><li class="chapter-item expanded "><a href="mouse.html"><strong aria-hidden="true">2.3.5.</strong> Mouse events</a></li></ol></li></ol></li><li class="chapter-item expanded "><a href="focus.html"><strong aria-hidden="true">3.</strong> Focus</a></li><li class="chapter-item expanded "><a href="bindings.html"><strong aria-hidden="true">4.</strong> Binding system</a></li><li class="chapter-item expanded "><a href="scripting.html"><strong aria-hidden="true">5.</strong> Scripting</a></li><li class="chapter-item expanded "><a href="styling.html"><strong aria-hidden="true">6.</strong> Themes and Styling</a></li><li class="chapter-item expanded "><a href="widgets.html"><strong aria-hidden="true">7.</strong> Widget Library</a></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString().split("#")[0].split("?")[0];
        if (current_page.endsWith("/")) {
            current_page += "index.html";
        }
        var links = Array.prototype.slice.call(this.querySelectorAll("a"));
        var l = links.length;
        for (var i = 0; i < l; ++i) {
            var link = links[i];
            var href = link.getAttribute("href");
            if (href && !href.startsWith("#") && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The "index" page is supposed to alias the first chapter in the book.
            if (link.href === current_page || (i === 0 && path_to_root === "" && current_page.endsWith("/index.html"))) {
                link.classList.add("active");
                var parent = link.parentElement;
                if (parent && parent.classList.contains("chapter-item")) {
                    parent.classList.add("expanded");
                }
                while (parent) {
                    if (parent.tagName === "LI" && parent.previousElementSibling) {
                        if (parent.previousElementSibling.classList.contains("chapter-item")) {
                            parent.previousElementSibling.classList.add("expanded");
                        }
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', function(e) {
            if (e.target.tagName === 'A') {
                sessionStorage.setItem('sidebar-scroll', this.scrollTop);
            }
        }, { passive: true });
        var sidebarScrollTop = sessionStorage.getItem('sidebar-scroll');
        sessionStorage.removeItem('sidebar-scroll');
        if (sidebarScrollTop) {
            // preserve sidebar scroll position when navigating via links within sidebar
            this.scrollTop = sidebarScrollTop;
        } else {
            // scroll sidebar to current active section when navigating via "next/previous chapter" buttons
            var activeSection = document.querySelector('#sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        var sidebarAnchorToggles = document.querySelectorAll('#sidebar a.toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(function (el) {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define("mdbook-sidebar-scrollbox", MDBookSidebarScrollbox);
