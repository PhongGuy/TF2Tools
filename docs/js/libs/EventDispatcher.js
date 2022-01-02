/**
 * @author mrdoob / http://mrdoob.com/
 */

const EventDispatcher = function () {};
Object.assign(EventDispatcher.prototype, {
  addEventListener: function (i, t) {
    void 0 === this._listeners && (this._listeners = {});
    const e = this._listeners;
    void 0 === e[i] && (e[i] = []), e[i].indexOf(t) === -1 && e[i].push(t);
  },
  hasEventListener: function (i, t) {
    if (void 0 === this._listeners) return !1;
    const e = this._listeners;
    return void 0 !== e[i] && e[i].indexOf(t) !== -1;
  },
  removeEventListener: function (i, t) {
    if (void 0 !== this._listeners) {
      const e = this._listeners[i];
      if (void 0 !== e) {
        const s = e.indexOf(t);
        s !== -1 && e.splice(s, 1);
      }
    }
  },
  dispatchEvent: function (i) {
    if (void 0 !== this._listeners) {
      const t = this._listeners[i.type];
      if (void 0 !== t) {
        i.target = this;
        const e = [];
        let s = 0;
        const n = t.length;
        for (s = 0; s < n; s++) e[s] = t[s];
        for (s = 0; s < n; s++) e[s].call(this, i);
      }
    }
  },
});
