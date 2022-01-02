(function (compodoc) {
  function LunrSearchEngine() {
    this.index = undefined;
    this.store = {};
    this.name = "LunrSearchEngine";
  }

  LunrSearchEngine.prototype.init = function () {
    const that = this;
    const d = new promise.Promise();

    that.index = lunr.Index.load(COMPODOC_SEARCH_INDEX.index);
    that.store = COMPODOC_SEARCH_INDEX.store;
    d.done();

    return d;
  };

  LunrSearchEngine.prototype.search = function (q, offset, length) {
    const that = this;
    let results = [];
    const d = new promise.Promise();

    if (this.index) {
      results = $.map(this.index.search("*" + q + "*"), function (result) {
        const doc = that.store[result.ref];

        return {
          title: doc.title,
          url: doc.url,
          body: doc.summary || doc.body,
        };
      });
    }

    d.done({
      query: q,
      results: length === 0 ? results : results.slice(0, length),
      count: results.length,
    });

    return d;
  };

  compodoc.addEventListener(compodoc.EVENTS.READY, function (event) {
    const engine = new LunrSearchEngine();
    let initialized = false;

    function query(q, offset, length) {
      if (!initialized) throw new Error("Search has not been initialized");
      return engine.search(q, offset, length);
    }

    compodoc.search = {
      query: query,
    };

    engine.init().then(function () {
      initialized = true;
      compodoc.dispatchEvent({
        type: compodoc.EVENTS.SEARCH_READY,
      });
    });
  });
})(compodoc);
