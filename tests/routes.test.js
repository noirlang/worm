import test from "node:test";
import assert from "node:assert";

// Mock Browser environment for import testing
const mockElement = {
  classList: {
    add: () => {},
    toggle: () => {},
    remove: () => {}
  },
  querySelectorAll: () => [],
  addEventListener: () => {},
  focus: () => {},
  dataset: {},
  set innerHTML(val) {},
  get innerHTML() { return ""; }
};

let clickListener = null;

globalThis.window = {
  location: {
    search: "?native=0&route=home"
  },
  addEventListener: () => {}
};
globalThis.location = globalThis.window.location;
if (typeof globalThis.navigator === "undefined") {
  globalThis.navigator = { userAgent: "Mozilla/5.0 (X11; Linux x86_64)", platform: "Linux x86_64" };
} else {
  try {
    Object.defineProperty(globalThis.navigator, "userAgent", { value: "Mozilla/5.0 (X11; Linux x86_64)", configurable: true });
    Object.defineProperty(globalThis.navigator, "platform", { value: "Linux x86_64", configurable: true });
  } catch (_) {}
}
globalThis.document = {
  documentElement: {
    classList: { add: () => {} },
    lang: "tr",
    set lang(v) {}
  },
  querySelector: () => mockElement,
  querySelectorAll: () => [],
  getElementById: () => mockElement,
  addEventListener: (event, callback) => {
    if (event === "click") {
      clickListener = callback;
    }
  }
};
globalThis.localStorage = {
  getItem: () => "tr",
  setItem: () => {}
};

test("Frontend Routing and Module Health", async (t) => {
  await t.test("pages modules can be imported and expose expected functions", async () => {
    const { homePage, metric } = await import("../ui/pages/home.js");
    assert.strictEqual(typeof homePage, "function", "homePage should be a function");
    assert.strictEqual(typeof metric, "function", "metric should be a function");

    const { workflowPage, pickerField, field, pageTitle } = await import("../ui/pages/workflow.js");
    assert.strictEqual(typeof workflowPage, "function", "workflowPage should be a function");
    assert.strictEqual(typeof field, "function", "field should be a function");

    const { androidPage, androidModePage } = await import("../ui/android.js");
    assert.strictEqual(typeof androidPage, "function", "androidPage should be a function");
    assert.strictEqual(typeof androidModePage, "function", "androidModePage should be a function");

    const { iosPage, handleIosAction } = await import("../ui/ios.js");
    assert.strictEqual(typeof iosPage, "function", "iosPage should be a function");
    assert.strictEqual(typeof handleIosAction, "function", "handleIosAction should be a function");
  });

  await t.test("icons module correctly hydrated and exports functions", async () => {
    const { icon, hydrateIcons, icons } = await import("../ui/icons.js");
    assert.strictEqual(typeof icon, "function", "icon should be a function");
    assert.strictEqual(typeof hydrateIcons, "function", "hydrateIcons should be a function");
    assert.ok(icons.home, "home icon path should exist");
  });

  await t.test("app.js initializes and executes without crashing", async () => {
    // This will import and immediately run the initialization scripts
    const appModule = await import("../ui/app.js");
    assert.ok(appModule, "app.js should be successfully imported");
  });

  await t.test("simulate navigation clicks through all routes without crashes", async () => {
    assert.ok(clickListener, "Click listener should be registered on document");

    const routesList = [
      "home", 
      "windows", 
      "linux", 
      "android", 
      "ios",
      "agent", 
      "analysis", 
      "other", 
      "settings", 
      "about", 
      "workflow:windows-remote-disk", 
      "android:logical"
    ];

    for (const route of routesList) {
      const mockEvent = {
        preventDefault: () => {},
        target: {
          closest: (selector) => {
            if (selector === "[data-route]") {
              return { dataset: { route } };
            }
            return null;
          }
        }
      };

      // This will invoke setRoute and render() in app.js
      assert.doesNotThrow(() => {
        clickListener(mockEvent);
      }, `Route "${route}" should navigate and render without throwing exceptions`);
    }
  });
});
