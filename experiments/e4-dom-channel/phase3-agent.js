// P3.1 transport prototype only. Page code can replace this object and its data.
globalThis.phase3Agent = {
  records: [],
  overflow: false,
  drain() {
    const result = JSON.stringify({records: this.records, overflow: this.overflow});
    this.records = [];
    return result;
  }
};
const phase3Observer = new MutationObserver(records => {
  for (const record of records) {
    if (phase3Agent.records.length === 64) {
      phase3Agent.overflow = true;
      phase3Observer.disconnect();
      return;
    }
    phase3Agent.records.push({type: record.type});
  }
});
phase3Observer.observe(document.body, {childList: true});
'observing';
