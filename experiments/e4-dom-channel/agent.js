// E4 probe only: this agent shares the page's JavaScript world.
globalThis.bulwarkProbe = {
  trivial() { return "bulwark-e4"; },
  subtree() {
    return JSON.stringify(Array.from(document.querySelectorAll("#fixture > span"),
      node => ({tag: node.tagName, id: node.id, text: node.textContent})));
  },
  send(id, value) {
    return fetch("/bridge/" + id + "?value=" + encodeURIComponent(value))
      .then(response => response.text());
  }
};
