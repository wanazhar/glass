pub(super) fn native_form_associated_elements_script() -> &'static str {
    r##"await (async () => {
      const errorName = action => {
        try { action(); return null; }
        catch (error) { return error.name; }
      };
      const lifecycleEvents = [];
      globalThis.__glassNativeFaceLifecycleEvents = lifecycleEvents;
      class ScalarField extends HTMLElement {
        static formAssociated = true;
        constructor() {
          super();
          this.internals = this.attachInternals();
          this.internals.setFormValue('one', 'state-one');
        }
      }
      customElements.define('x-scalar-field', ScalarField);

      class PairField extends HTMLElement {
        static formAssociated = true;
        constructor() {
          super();
          this.internals = this.attachInternals();
          const submitted = new FormData();
          submitted.append('first', 'alpha');
          submitted.append('second', 'beta');
          submitted.append('proof', new File(['payload'], 'proof.txt', { type: 'text/plain' }));
          this.internals.setFormValue(submitted, 'state-pair');
          submitted.set('first', 'later mutation');
        }
      }
      customElements.define('x-pair-field', PairField);

      class EmptyField extends HTMLElement {
        static formAssociated = true;
        constructor() { super(); this.attachInternals().setFormValue(null); }
      }
      customElements.define('x-empty-field', EmptyField);

      class DisabledField extends HTMLElement {
        static formAssociated = true;
        constructor() { super(); this.attachInternals().setFormValue('must-not-submit'); }
      }
      customElements.define('x-disabled-field', DisabledField);

      class LegendField extends HTMLElement {
        static formAssociated = true;
        constructor() { super(); this.attachInternals().setFormValue('legend'); }
      }
      customElements.define('x-legend-field', LegendField);

      class DisabledFieldsetControl extends HTMLElement {
        static formAssociated = true;
        constructor() { super(); this.attachInternals().setFormValue('fieldset-disabled'); }
      }
      customElements.define('x-fieldset-disabled', DisabledFieldsetControl);

      class UnnamedField extends HTMLElement {
        static formAssociated = true;
        constructor() { super(); this.attachInternals().setFormValue('must-not-submit'); }
      }
      customElements.define('x-unnamed-field', UnnamedField);

      class ExternalField extends HTMLElement {
        static formAssociated = true;
        constructor() { super(); this.internals = this.attachInternals(); this.internals.setFormValue('outside'); }
      }
      customElements.define('x-external-field', ExternalField);

      class LifecycleField extends HTMLElement {
        static formAssociated = true;
        constructor() { super(); this.internals = this.attachInternals(); }
        formAssociatedCallback(form) {
          lifecycleEvents.push(['owner', this.id, form === null ? null : form.id]);
        }
        formDisabledCallback(disabled) {
          lifecycleEvents.push(['disabled', this.id, disabled]);
        }
      }
      customElements.define('x-face-lifecycle', LifecycleField);
      const initialLifecycleEvents = lifecycleEvents.slice();

      const main = document.getElementById('main-form');
      const external = document.getElementById('external-form');
      const scalar = document.getElementById('scalar');
      const externalField = document.getElementById('external-field');
      const explicitLifecycle = document.getElementById('face-explicit');
      const disabledFieldset = document.getElementById('face-fieldset');
      const disabledLifecycle = document.getElementById('face-disabled');
      const legendLifecycle = document.getElementById('face-legend');
      external.setAttribute('id', 'renamed-form');
      external.setAttribute('id', 'external-form');
      const duplicateId = document.createElement('div');
      duplicateId.setAttribute('id', 'external-form');
      document.body.insertBefore(duplicateId, external);
      duplicateId.remove();
      explicitLifecycle.setAttribute('form', 'main-form');
      const faceOwner = document.getElementById('face-owner');
      faceOwner.setAttribute('disabled', '');
      faceOwner.removeAttribute('disabled');
      disabledFieldset.removeAttribute('disabled');
      disabledFieldset.setAttribute('disabled', '');
      const legend = legendLifecycle.parentElement;
      disabledFieldset.appendChild(legendLifecycle);
      legend.appendChild(legendLifecycle);
      disabledLifecycle.remove();
      main.appendChild(disabledLifecycle);
      const currentLifecycleEvents = lifecycleEvents.slice();
      const setterErrors = {
        missingValue: errorName(() => scalar.internals.setFormValue()),
        symbolValue: errorName(() => scalar.internals.setFormValue(Symbol('invalid'))),
      };
      const forgedMarkers = (() => {
        scalar.internals.setFormValue({ __glassFormData: true, _entries: [['forged', 'entry']] });
        const formDataMarkerValue = new FormData(main).get('scalar');
        const forgedEntryWasExpanded = new FormData(main).has('forged');
        scalar.internals.setFormValue({ __glassNativeFile: true, name: 'forged.txt' });
        const fileMarkerValue = new FormData(main).get('scalar');
        scalar.internals.setFormValue('one');
        return [formDataMarkerValue, forgedEntryWasExpanded, fileMarkerValue];
      })();
      const formData = new FormData(main);

      class OrdinaryElement extends HTMLElement {
        constructor() { super(); this.internals = this.attachInternals(); }
      }
      customElements.define('x-ordinary-element', OrdinaryElement);
      const ordinary = document.createElement('x-ordinary-element');

      class DisabledInternalsElement extends HTMLElement {
        static disabledFeatures = ['internals'];
      }
      customElements.define('x-disabled-internals', DisabledInternalsElement);

      class BuiltinElement extends HTMLButtonElement {
        static formAssociated = true;
      }
      customElements.define('x-builtin-element', BuiltinElement, { extends: 'button' });
      const customizedBuiltin = document.createElement('button', { is: 'x-builtin-element' });

      class CallbackElement extends HTMLElement {
        static formAssociated = true;
        formResetCallback() {}
      }
      customElements.define('x-callback-element', CallbackElement);
      class StateRestoreCallbackElement extends HTMLElement {
        static formAssociated = true;
        formStateRestoreCallback() {}
      }
      class InvalidCallbackElement extends HTMLElement {
        static formAssociated = true;
      }
      InvalidCallbackElement.prototype.formDisabledCallback = {};

      return {
        brands: [scalar.internals instanceof ElementInternals, scalar.internals.form === main,
          externalField.internals.form === external],
        entries: Array.from(formData.entries(), ([name, value]) => [
          name,
          value instanceof File ? ['file', value.name, value.type] : value,
        ]),
        forgedMarkers,
        lifecycle: {
          initial: initialLifecycleEvents,
          transitions: currentLifecycleEvents.slice(initialLifecycleEvents.length),
          explicitOwner: explicitLifecycle.internals.form.id,
        },
        externalEntries: Array.from(new FormData(external).entries()),
        errors: {
          directConstructor: errorName(() => new ElementInternals()),
          ordinaryElement: errorName(() => document.createElement('div').attachInternals()),
          duplicateAttachment: errorName(() => scalar.attachInternals()),
          nonFormAssociatedSetter: errorName(() => ordinary.internals.setFormValue('bad')),
          nonFormAssociatedGetter: errorName(() => ordinary.internals.form),
          disabledInternals: errorName(() => document.createElement('x-disabled-internals').attachInternals()),
          customizedBuiltin: errorName(() => customizedBuiltin.attachInternals()),
          resetCallbackPublished: customElements.get('x-callback-element') === CallbackElement,
          unsupportedStateRestoreCallback: errorName(() => customElements.define('x-state-restore-callback', StateRestoreCallbackElement)),
          stateRestoreCallbackNotPublished: customElements.get('x-state-restore-callback') === undefined,
          invalidLifecycleCallback: errorName(() => customElements.define('x-invalid-lifecycle', InvalidCallbackElement)),
          ...setterErrors,
        },
      };
    })()"##
}

pub(super) fn assert_native_form_associated_elements_result(value: &serde_json::Value) {
    assert_eq!(
        value,
        &serde_json::json!({
            "brands": [true, true, true],
            "entries": [
                ["scalar", "one"],
                ["builtin", "native"],
                ["first", "alpha"],
                ["second", "beta"],
                ["proof", ["file", "proof.txt", "text/plain"]],
                ["legend", "legend"],
            ],
            "externalEntries": [["outside", "outside"]],
            "forgedMarkers": ["[object Object]", false, "[object Object]"],
            "lifecycle": {
                "initial": [
                    ["owner", "face-owner", "main-form"],
                    ["owner", "face-legend", "main-form"],
                    ["owner", "face-disabled", "main-form"],
                    ["disabled", "face-disabled", true],
                    ["owner", "face-explicit", "external-form"],
                ],
                "transitions": [
                    ["owner", "face-explicit", null],
                    ["owner", "face-explicit", "external-form"],
                    ["owner", "face-explicit", null],
                    ["owner", "face-explicit", "external-form"],
                    ["owner", "face-explicit", "main-form"],
                    ["disabled", "face-owner", true],
                    ["disabled", "face-owner", false],
                    ["disabled", "face-disabled", false],
                    ["disabled", "face-disabled", true],
                    ["disabled", "face-legend", true],
                    ["disabled", "face-legend", false],
                    ["owner", "face-disabled", null],
                    ["disabled", "face-disabled", false],
                    ["owner", "face-disabled", "main-form"],
                ],
                "explicitOwner": "main-form",
            },
            "errors": {
                "directConstructor": "TypeError",
                "ordinaryElement": "NotSupportedError",
                "duplicateAttachment": "NotSupportedError",
                "nonFormAssociatedSetter": "NotSupportedError",
                "nonFormAssociatedGetter": "NotSupportedError",
                "disabledInternals": "NotSupportedError",
                "customizedBuiltin": "NotSupportedError",
                "resetCallbackPublished": true,
                "unsupportedStateRestoreCallback": "NotSupportedError",
                "stateRestoreCallbackNotPublished": true,
                "invalidLifecycleCallback": "TypeError",
                "missingValue": "TypeError",
                "symbolValue": "TypeError",
            },
        })
    );
}

pub(super) fn native_form_associated_entries_after_script_refresh() -> &'static str {
    r##"(() => {
      const form = document.getElementById('main-form');
      const entries = Array.from(new FormData(form).entries(), ([name, value]) => [
        name,
        value instanceof File ? ['file', value.name, value.type] : value,
      ]);
      return {
        entries,
        internalsBrand: document.getElementById('scalar').internals instanceof ElementInternals,
        formOwner: document.getElementById('scalar').internals.form === form,
        lifecycleEventCount: globalThis.__glassNativeFaceLifecycleEvents.length,
        explicitLifecycleOwner: document.getElementById('face-explicit').internals.form.id,
      };
    })()"##
}

pub(super) fn native_form_reset_script() -> &'static str {
    r##"(() => {
      const trace = [];
      globalThis.__glassNativeFormResetTrace = trace;
      class ResetField extends HTMLElement {
        static formAssociated = true;
        constructor() {
          super();
          this.internals = this.attachInternals();
          this.internals.setFormValue('before');
        }
        formResetCallback() {
          const text = document.getElementById('reset-text');
          trace.push(['face-reset', text.value]);
          this.internals.setFormValue('after');
          text.value += ':callback';
        }
      }
      customElements.define('x-reset-field', ResetField);
      class ThrowingResetField extends HTMLElement {
        static formAssociated = true;
        formResetCallback() {
          trace.push(['face-reset-throws']);
          throw new Error('reset callback failure');
        }
      }
      class LaterResetField extends HTMLElement {
        static formAssociated = true;
        formResetCallback() { trace.push(['face-reset-after-error']); }
      }
      customElements.define('x-reset-throwing-field', ThrowingResetField);
      customElements.define('x-reset-later-field', LaterResetField);

      const form = document.getElementById('reset-form');
      const text = document.getElementById('reset-text');
      const checkbox = document.getElementById('reset-checkbox');
      const textarea = document.getElementById('reset-textarea');
      const select = document.getElementById('reset-select');
      const fallbackSelect = document.getElementById('reset-fallback-select');
      const file = document.getElementById('reset-file');
      const external = document.getElementById('reset-external');
      const face = document.getElementById('reset-face');
      const resetEvents = [];
      form.addEventListener('reset', event => {
        resetEvents.push([event.bubbles, event.cancelable]);
        text.value = 'listener-value';
      }, { once: true });
      text.value = 'dirty-value';
      text.setAttribute('value', 'current-default');
      checkbox.checked = false;
      textarea.value = 'dirty-notes';
      select.options[0].selected = true;
      fallbackSelect.options[0].selected = true;
      external.value = 'dirty-external';
      text.setCustomValidity('author validity survives reset');
      const firstReturn = form.reset();
      const first = {
        returnValueIsUndefined: firstReturn === undefined,
        text: text.value,
        checked: checkbox.checked,
        textarea: textarea.value,
        select: select.value,
        options: select.options.map(option => option.selected),
        fallbackSelect: fallbackSelect.value,
        fallbackOptions: fallbackSelect.options.map(option => option.selected),
        fileCount: file.files.length,
        fileValue: file.value,
        customError: text.validity.customError,
        unnamedRadios: ['reset-unnamed-a', 'reset-unnamed-b'].map(id => document.getElementById(id).checked),
        namedRadios: ['reset-named-a', 'reset-named-b'].map(id => document.getElementById(id).checked),
        external: external.value,
        faceValue: new FormData(form).get('face'),
        events: resetEvents.slice(),
        callbacks: trace.slice(),
      };

      let nestedCalls = 0;
      form.addEventListener('reset', () => {
        nestedCalls += 1;
        form.reset();
      }, { once: true });
      const nestedReturn = form.reset();
      text.value = 'cancelled-value';
      form.addEventListener('reset', event => event.preventDefault(), { once: true });
      const cancelledReturn = form.reset();
      return {
        first,
        nested: {
          returnValueIsUndefined: nestedReturn === undefined,
          listenerCalls: nestedCalls,
          text: text.value,
          callbacks: trace.slice(),
        },
        cancelled: {
          returnValueIsUndefined: cancelledReturn === undefined,
          text: text.value,
          callbacks: trace.slice(),
          faceValue: new FormData(form).get('face'),
        },
        faceOwner: face.internals.form === form,
      };
    })()"##
}

pub(super) fn native_form_reset_after_script_refresh() -> &'static str {
    r##"(() => {
      const form = document.getElementById('reset-form');
      const select = document.getElementById('reset-select');
      const fallbackSelect = document.getElementById('reset-fallback-select');
      const file = document.getElementById('reset-file');
      return {
        text: document.getElementById('reset-text').value,
        checked: document.getElementById('reset-checkbox').checked,
        textarea: document.getElementById('reset-textarea').value,
        select: select.value,
        options: select.options.map(option => option.selected),
        fallbackSelect: fallbackSelect.value,
        fallbackOptions: fallbackSelect.options.map(option => option.selected),
        fileCount: file.files.length,
        fileValue: file.value,
        customError: document.getElementById('reset-text').validity.customError,
        unnamedRadios: ['reset-unnamed-a', 'reset-unnamed-b'].map(id => document.getElementById(id).checked),
        namedRadios: ['reset-named-a', 'reset-named-b'].map(id => document.getElementById(id).checked),
        external: document.getElementById('reset-external').value,
        faceValue: new FormData(form).get('face'),
        callbackCount: globalThis.__glassNativeFormResetTrace.length,
      };
    })()"##
}

pub(super) fn assert_native_form_reset_result(value: &serde_json::Value) {
    assert_eq!(
        value,
        &serde_json::json!({
            "first": {
                "returnValueIsUndefined": true,
                "text": "current-default:callback",
                "checked": true,
                "textarea": "default-notes",
                "select": "default-choice",
                "options": [false, true],
                "fallbackSelect": "fallback-enabled",
                "fallbackOptions": [false, true],
                "fileCount": 0,
                "fileValue": "",
                "customError": true,
                "unnamedRadios": [true, true],
                "namedRadios": [false, true],
                "external": "external-default",
                "faceValue": "after",
                "events": [[true, true]],
                "callbacks": [
                    ["face-reset", "current-default"],
                    ["face-reset-throws"],
                    ["face-reset-after-error"],
                ],
            },
            "nested": {
                "returnValueIsUndefined": true,
                "listenerCalls": 1,
                "text": "cancelled-value",
                "callbacks": [
                    ["face-reset", "current-default"],
                    ["face-reset-throws"],
                    ["face-reset-after-error"],
                    ["face-reset", "current-default"],
                    ["face-reset-throws"],
                    ["face-reset-after-error"],
                ],
            },
            "cancelled": {
                "returnValueIsUndefined": true,
                "text": "cancelled-value",
                "callbacks": [
                    ["face-reset", "current-default"],
                    ["face-reset-throws"],
                    ["face-reset-after-error"],
                    ["face-reset", "current-default"],
                    ["face-reset-throws"],
                    ["face-reset-after-error"],
                ],
                "faceValue": "after",
            },
            "faceOwner": true,
        })
    );
}

pub(super) fn native_form_reset_fixture() -> &'static str {
    "<!doctype html><html><body><form id='reset-form'><input id='reset-text' name='text' value='initial'><input id='reset-checkbox' name='checked' type='checkbox' checked><textarea id='reset-textarea' name='notes'>default-notes</textarea><select id='reset-select' name='choice'><option value='first-choice'>First</option><option value='default-choice' selected>Default</option></select><select id='reset-fallback-select' name='fallback'><option value='fallback-disabled' disabled>Disabled</option><option value='fallback-enabled'>Enabled</option></select><input id='reset-file' type='file' name='file'><input id='reset-unnamed-a' type='radio' checked><input id='reset-unnamed-b' type='radio' checked><input id='reset-named-a' type='radio' name='named-reset' checked><input id='reset-named-b' type='radio' name='named-reset' checked><x-reset-field id='reset-face' name='face'></x-reset-field><x-reset-throwing-field></x-reset-throwing-field><x-reset-later-field></x-reset-later-field></form><input id='reset-external' form='reset-form' name='external' value='external-default'></body></html>"
}

pub(super) fn native_reset_button_fixture() -> &'static str {
    "<!doctype html><html><body><form id='main-form' action='/submitted'><input id='main-value' name='value' value='main-default'><input id='reset-required' required><input id='input-reset' type='reset' value='Reset input'><button id='button-reset' type='reset'>Reset button</button><button id='click-cancel' type='reset'>Cancel click</button><button id='promote-reset' type='button' form='main-form'>Promote</button></form><form id='other-form' action='/other-submitted'><input id='other-value' name='other' value='other-default'><input id='other-required' required></form></body></html>"
}

pub(super) fn native_reset_button_setup_script() -> &'static str {
    r##"(() => {
      const main = document.getElementById('main-form');
      const other = document.getElementById('other-form');
      const value = document.getElementById('main-value');
      const clickCancel = document.getElementById('click-cancel');
      const promote = document.getElementById('promote-reset');
      const trace = { mainReset: 0, otherReset: 0, submit: 0, invalid: 0 };
      globalThis.__glassResetButtonTrace = trace;
      globalThis.__glassResetButtonEvents = [];
      globalThis.__glassCancelKeyboardEnter = false;
      globalThis.__glassResetButtonInitialHref = location.href;
      main.addEventListener('reset', () => { trace.mainReset += 1; });
      other.addEventListener('reset', event => {
        trace.otherReset += 1;
        event.preventDefault();
      });
      for (const form of [main, other]) {
        form.addEventListener('submit', event => {
          trace.submit += 1;
          event.preventDefault();
        });
      }
      for (const id of ['reset-required', 'other-required']) {
        document.getElementById(id).addEventListener('invalid', () => { trace.invalid += 1; });
      }
      for (const id of ['input-reset', 'button-reset', 'click-cancel']) {
        const control = document.getElementById(id);
        control.addEventListener('keydown', event => {
          globalThis.__glassResetButtonEvents.push('keydown:' + id + ':' + event.key);
          if (id === 'input-reset' && globalThis.__glassCancelKeyboardEnter && event.key === 'Enter') {
            event.preventDefault();
          }
        });
        control.addEventListener('keyup', event => {
          globalThis.__glassResetButtonEvents.push('keyup:' + id + ':' + event.key);
        });
        control.addEventListener('click', () => {
          globalThis.__glassResetButtonEvents.push('click:' + id);
        });
      }
      clickCancel.addEventListener('click', event => event.preventDefault());
      promote.addEventListener('click', () => {
        promote.setAttribute('type', 'reset');
        promote.setAttribute('form', 'other-form');
      });
      value.value = 'dirty-script';
      document.getElementById('button-reset').click();
      return true;
    })()"##
}

pub(super) fn native_reset_button_state_script() -> &'static str {
    r##"(() => ({
      mainValue: document.getElementById('main-value').value,
      otherValue: document.getElementById('other-value').value,
      ...globalThis.__glassResetButtonTrace,
      sameHref: location.href === globalThis.__glassResetButtonInitialHref,
    }))()"##
}

pub(super) fn native_form_associated_focus_markup() -> &'static str {
    "<div id='zero-first' tabindex='0'></div><x-focus-field id='enabled' tabindex='1'></x-focus-field><x-focus-field id='own-disabled' disabled tabindex='1'></x-focus-field><fieldset disabled><legend><x-focus-field id='legend-enabled' tabindex='2'></x-focus-field></legend><x-focus-field id='fieldset-disabled' tabindex='2'></x-focus-field></fieldset><x-plain-field id='ordinary-disabled' disabled tabindex='3'></x-plain-field>"
}

pub(super) fn native_form_associated_focus_definition() -> &'static str {
    r##"class FocusField extends HTMLElement {
      static formAssociated = true;
      constructor() { super(); this.attachInternals(); }
      get disabled() { return false; }
    }
    customElements.define('x-focus-field', FocusField);
    class PlainField extends HTMLElement {
      get disabled() { return false; }
    }
    customElements.define('x-plain-field', PlainField);
    true"##
}

pub(super) fn native_inert_focus_markup() -> &'static str {
    "<button id='first' tabindex='1'>First</button><section id='panel' inert><button id='implicit-blocked'>Implicit</button><div id='explicit-blocked' tabindex='2'></div></section><div id='natural' tabindex='0'></div>"
}

pub(super) fn native_focus_event_markup() -> &'static str {
    "<div id='outer'><button id='first'>First</button><button id='second'>Second</button></div>"
}

pub(super) fn native_focus_event_listener_script() -> &'static str {
    r#"(() => {
      globalThis.focusEvents = [];
      globalThis.idlFocusEvents = [];
      globalThis.idlFocusOrder = [];
      const outer = document.getElementById('outer');
      const first = document.getElementById('first');
      const second = document.getElementById('second');
      const record = event => focusEvents.push([
        event.type,
        event.target.id,
        event.relatedTarget ? event.relatedTarget.id : null,
        event instanceof FocusEvent,
      ]);
      for (const type of ['blur', 'focus']) outer.addEventListener(type, record, true);
      for (const type of ['focusout', 'focusin']) outer.addEventListener(type, record);
      first.addEventListener('focus', () => idlFocusOrder.push('before'));
      first.onfocus = () => {
        idlFocusEvents.push(['replaced']);
        idlFocusOrder.push('replaced');
      };
      first.addEventListener('focus', () => idlFocusOrder.push('after'));
      first.onfocus = function(event) {
        idlFocusEvents.push(['focus-first', this === event.currentTarget, event.target === event.currentTarget]);
        idlFocusOrder.push('handler');
      };
      first.onblur = () => idlFocusEvents.push(['replaced-blur']);
      first.onblur = function(event) {
        idlFocusEvents.push(['blur-first', this === event.currentTarget, event.target === event.currentTarget]);
      };
      second.onfocus = function(event) {
        idlFocusEvents.push(['focus-second', this === event.currentTarget, event.target === event.currentTarget]);
      };
      second.onblur = function(event) {
        idlFocusEvents.push(['blur-second', this === event.currentTarget, event.target === event.currentTarget]);
      };
      first.focus();
      second.focus();
      first.onfocus = null;
      second.onblur = null;
      globalThis.clearedFocusHandlers = [first.onfocus, second.onblur];
      return focusEvents;
    })()"#
}

pub(super) fn native_focus_transition_events() -> serde_json::Value {
    serde_json::json!([
        ["focus", "first", null, true],
        ["focusin", "first", null, true],
        ["blur", "first", "second", true],
        ["focusout", "first", "second", true],
        ["focus", "second", "first", true],
        ["focusin", "second", "first", true],
    ])
}

const NATIVE_GLOBAL_EVENT_HANDLER_TYPES: &[&str] = &[
    "abort",
    "auxclick",
    "beforeinput",
    "beforematch",
    "beforetoggle",
    "blur",
    "cancel",
    "canplay",
    "canplaythrough",
    "change",
    "click",
    "close",
    "command",
    "contextlost",
    "contextmenu",
    "contextrestored",
    "copy",
    "cuechange",
    "cut",
    "dblclick",
    "drag",
    "dragend",
    "dragenter",
    "dragleave",
    "dragover",
    "dragstart",
    "drop",
    "durationchange",
    "emptied",
    "ended",
    "error",
    "focus",
    "formdata",
    "input",
    "invalid",
    "keydown",
    "keypress",
    "keyup",
    "load",
    "loadeddata",
    "loadedmetadata",
    "loadstart",
    "mousedown",
    "mouseenter",
    "mouseleave",
    "mousemove",
    "mouseout",
    "mouseover",
    "mouseup",
    "paste",
    "pause",
    "play",
    "playing",
    "progress",
    "ratechange",
    "reset",
    "resize",
    "scroll",
    "scrollend",
    "securitypolicyviolation",
    "seeked",
    "seeking",
    "select",
    "slotchange",
    "stalled",
    "submit",
    "suspend",
    "timeupdate",
    "toggle",
    "volumechange",
    "waiting",
    "webkitanimationend",
    "webkitanimationiteration",
    "webkitanimationstart",
    "webkittransitionend",
    "wheel",
];

const NATIVE_WINDOW_EVENT_HANDLER_TYPES: &[&str] = &[
    "afterprint",
    "beforeprint",
    "beforeunload",
    "hashchange",
    "languagechange",
    "message",
    "messageerror",
    "offline",
    "online",
    "pagehide",
    "pagereveal",
    "pageshow",
    "pageswap",
    "popstate",
    "rejectionhandled",
    "storage",
    "unhandledrejection",
    "unload",
];

const NATIVE_DOCUMENT_EVENT_HANDLER_TYPES: &[&str] = &["readystatechange", "visibilitychange"];

pub(super) fn native_html_global_event_handler_surface_script() -> String {
    let types = serde_json::to_string(NATIVE_GLOBAL_EVENT_HANDLER_TYPES).unwrap();
    format!(
        "(() => {{ const element = document.getElementById('first') || document.getElementById('child').contentDocument.getElementById('first'); const types = {types}; return [types.filter(type => !(\"on\" + type in element)), types.filter(type => element[\"on\" + type] !== null), types.filter(type => Object.prototype.hasOwnProperty.call(element, \"on\" + type))]; }})()"
    )
}

pub(super) fn native_window_document_event_handler_surface_script() -> String {
    let global_types = serde_json::to_string(NATIVE_GLOBAL_EVENT_HANDLER_TYPES).unwrap();
    let window_types = serde_json::to_string(NATIVE_WINDOW_EVENT_HANDLER_TYPES).unwrap();
    let document_types = serde_json::to_string(NATIVE_DOCUMENT_EVENT_HANDLER_TYPES).unwrap();
    format!(
        r#"(() => {{
          const globalTypes = {global_types};
          const windowTypes = {window_types};
          const documentTypes = {document_types};
          const missing = (target, types) => types.filter(type => !("on" + type in target));
          const nonNull = (target, types) => types.filter(type => target["on" + type] !== null);
          const windowSurface = [...globalTypes, ...windowTypes];
          const documentSurface = [...globalTypes, ...documentTypes];
          const bodySurface = [...globalTypes, ...windowTypes];
          const body = document.body;
          const frameset = document.createElement("frameset");
          const generic = document.createElement("div");
          return [
            missing(window, windowSurface),
            nonNull(window, windowSurface),
            missing(document, documentSurface),
            nonNull(document, documentSurface),
            missing(body, bodySurface),
            nonNull(body, bodySurface),
            missing(frameset, bodySurface),
            nonNull(frameset, bodySurface),
            windowTypes.filter(type => ("on" + type) in generic),
            [frameset instanceof HTMLFrameSetElement, body instanceof HTMLBodyElement],
          ];
        }})()"#
    )
}

pub(super) fn native_window_document_event_handler_dispatch_script() -> &'static str {
    r#"(() => {
      const events = [];
      const body = document.body;
      const frameset = document.createElement('frameset');
      window.onmessage = function(event) {
        events.push(['window-message', this === window, event.target === window, event.currentTarget === window]);
      };
      window.dispatchEvent(new Event('message'));
      const readyHandler = function(event) {
        events.push(['document-ready', this === document, event.target === document, event.currentTarget === document]);
      };
      document.onreadystatechange = readyHandler;
      document.dispatchEvent(new Event('readystatechange'));
      document.onvisibilitychange = function(event) {
        events.push(['document-visibility', this === document, event.target === document, event.currentTarget === document]);
      };
      document.dispatchEvent(new Event('visibilitychange'));
      const bodyLoad = function(event) {
        events.push(['body-load', this === window, event.target === window, event.currentTarget === window]);
      };
      body.onload = bodyLoad;
      const bodyLoadAliasesWindow = window.onload === bodyLoad && body.onload === bodyLoad;
      window.dispatchEvent(new Event('load'));
      body.onload = null;
      const bodyLoadCleared = window.onload === null && body.onload === null;
      const framesetAfterPrint = function(event) {
        events.push(['frameset-afterprint', this === window, event.target === window, event.currentTarget === window]);
      };
      frameset.onafterprint = framesetAfterPrint;
      const framesetAliasesWindow = window.onafterprint === framesetAfterPrint;
      window.dispatchEvent(new Event('afterprint'));
      frameset.onafterprint = null;
      body.onclick = function(event) {
        events.push(['body-click', this === body, event.target === body, event.currentTarget === body]);
      };
      const bodyClickStayedLocal = window.onclick === null;
      body.dispatchEvent(new Event('click', { bubbles: true }));
      body.onclick = null;
      window.onmessage = null;
      document.onreadystatechange = null;
      document.onvisibilitychange = null;
      return [events, bodyLoadAliasesWindow, bodyLoadCleared, framesetAliasesWindow, bodyClickStayedLocal];
    })()"#
}

pub(super) fn native_window_routed_content_attribute_script() -> &'static str {
    r#"(() => {
      const order = globalThis.routedContentAttributeOrder = [];
      const body = document.body;
      body.setAttribute('onmessage', "globalThis.routedContentAttributeOrder.push('body-initial')");
      const initialAlias = typeof body.onmessage === 'function' && body.onmessage === window.onmessage;
      const tail = () => order.push('tail');
      window.addEventListener('message', tail);
      window.onmessage = () => order.push('idl');
      window.dispatchEvent(new Event('message'));
      const idlAlias = body.onmessage === window.onmessage;
      const idlOrder = order.slice();
      body.setAttribute('onmessage', "globalThis.routedContentAttributeOrder.push('body-replacement')");
      window.dispatchEvent(new Event('message'));
      const attributeAlias = body.onmessage === window.onmessage;
      const attributeOrder = order.slice();
      body.removeAttribute('onmessage');
      const removedAlias = window.onmessage === null && body.onmessage === null;
      window.dispatchEvent(new Event('message'));
      const afterRemoveOrder = order.slice();
      const frameset = document.createElement('frameset');
      frameset.setAttribute('onmessage', "globalThis.routedContentAttributeOrder.push('frameset')");
      const framesetAlias = frameset.onmessage === window.onmessage;
      window.dispatchEvent(new Event('message'));
      const framesetOrder = order.slice();
      frameset.removeAttribute('onmessage');
      const framesetCleared = frameset.onmessage === null && window.onmessage === null;
      window.removeEventListener('message', tail);
      const clicks = globalThis.routedContentAttributeClicks = [];
      body.setAttribute('onclick', "globalThis.routedContentAttributeClicks.push([this === document.body, event.currentTarget === document.body])");
      body.dispatchEvent(new Event('click'));
      body.removeAttribute('onclick');
      return [initialAlias, idlAlias, idlOrder, attributeAlias, attributeOrder,
        removedAlias, afterRemoveOrder, framesetAlias, framesetOrder,
        framesetCleared, clicks];
    })()"#
}

pub(super) fn native_window_onerror_return_semantics_script() -> &'static str {
    r#"(() => {
      const error = new Error('reported value');
      const argumentsSeen = [];
      window.onerror = function(message, filename, lineno, colno, reportedError) {
        argumentsSeen.push([this === window, message, filename, lineno, colno, reportedError === error]);
        return true;
      };
      const specialEvent = new ErrorEvent('error', {
        message: 'reported value', filename: 'native.js', lineno: 3, colno: 7, error,
      });
      const specialResult = window.dispatchEvent(specialEvent);
      const specialCase = [specialEvent.cancelable, specialEvent.defaultPrevented, specialResult, argumentsSeen[0]];

      window.onerror = () => false;
      const falseEvent = new ErrorEvent('error', { cancelable: true });
      const falseResult = window.dispatchEvent(falseEvent);
      const falseCase = [falseResult, falseEvent.defaultPrevented];

      let plainArguments;
      window.onerror = function(event) {
        plainArguments = [event instanceof Event, event.type, this === window];
        return true;
      };
      const plainEvent = new Event('error', { cancelable: true });
      const plainResult = window.dispatchEvent(plainEvent);
      const plainCase = [plainResult, plainEvent.defaultPrevented, plainArguments];

      const body = document.body;
      body.setAttribute('onerror', 'globalThis.bodyErrorThis = this === window; return true;');
      const attributeAliasesWindow = body.onerror === window.onerror;
      const attributeEvent = new ErrorEvent('error', { cancelable: true });
      const attributeResult = window.dispatchEvent(attributeEvent);
      const attributeCase = [
        attributeAliasesWindow,
        attributeResult,
        attributeEvent.defaultPrevented,
        globalThis.bodyErrorThis,
      ];
      body.removeAttribute('onerror');
      window.onerror = null;
      return [specialCase, falseCase, plainCase, attributeCase];
    })()"#
}

pub(super) fn native_beforeunload_return_semantics_script() -> &'static str {
    r#"(() => {
      const observed = [];
      const observer = event => observed.push(event);
      window.addEventListener('beforeunload', observer);
      const dispatchHost = () => {
        const result = globalThis.__glassDispatchHostEvents([{
          node_index: 4294967295,
          type: 'beforeunload',
          bubbles: false,
          cancelable: true,
        }])[0];
        const event = observed.pop();
        return [result, event instanceof BeforeUnloadEvent, event.cancelable,
          event.returnValue, event.defaultPrevented];
      };

      window.onbeforeunload = function() {
        globalThis.beforeUnloadHandlerThis = this === window;
        return 42;
      };
      const numericReturn = dispatchHost();
      window.onbeforeunload = () => null;
      const nullReturn = dispatchHost();
      window.onbeforeunload = () => undefined;
      const undefinedReturn = dispatchHost();
      window.onbeforeunload = () => '';
      const emptyReturn = dispatchHost();
      window.onbeforeunload = () => false;
      const falseReturn = dispatchHost();
      window.onbeforeunload = event => {
        event.returnValue = 'preserved';
        return 'replacement';
      };
      const preservedReturnValue = dispatchHost();
      window.onbeforeunload = event => {
        event.returnValue = 23;
        return null;
      };
      const explicitlyConvertedReturnValue = dispatchHost();

      window.onbeforeunload = () => 'not special for a plain Event';
      const plainEvent = new Event('beforeunload', { cancelable: true });
      const plainEventResult = window.dispatchEvent(plainEvent);
      const plainEventCase = [plainEventResult, plainEvent.defaultPrevented];
      window.onbeforeunload = () => false;
      const plainFalseEvent = new Event('beforeunload', { cancelable: true });
      const plainFalseResult = window.dispatchEvent(plainFalseEvent);
      const plainFalseCase = [plainFalseResult, plainFalseEvent.defaultPrevented];

      const body = document.body;
      body.setAttribute('onbeforeunload',
        'globalThis.beforeUnloadAttributeThis = this === window; return 7;');
      const bodyAlias = body.onbeforeunload === window.onbeforeunload;
      const bodyAttributeReturn = dispatchHost();
      body.removeAttribute('onbeforeunload');
      window.removeEventListener('beforeunload', observer);
      window.onbeforeunload = function() {
        globalThis.warningRan = 'yes';
        return 'site-authored warning';
      };
      return [numericReturn, nullReturn, undefinedReturn, emptyReturn, falseReturn,
        preservedReturnValue, explicitlyConvertedReturnValue,
        globalThis.beforeUnloadHandlerThis, bodyAlias, bodyAttributeReturn,
        globalThis.beforeUnloadAttributeThis, plainEventCase, plainFalseCase];
    })()"#
}

pub(super) fn native_event_handler_false_cancellation_script() -> &'static str {
    r#"(() => {
      const windowOrder = [];
      window.onmessage = function() {
        windowOrder.push(['handler', this === window]);
        return false;
      };
      const windowTail = event => windowOrder.push(['later', event.defaultPrevented]);
      window.addEventListener('message', windowTail);
      const messageEvent = new Event('message');
      const messageResult = window.dispatchEvent(messageEvent);
      window.removeEventListener('message', windowTail);
      window.onmessage = null;

      const attributeCalls = globalThis.nativeAttrFalseCalls = [];
      const attributeOrder = [];
      const button = document.createElement('button');
      button.setAttribute('onclick',
        'globalThis.nativeAttrFalseCalls.push(this === event.currentTarget); return false;');
      button.addEventListener('click', event => attributeOrder.push(['later', event.defaultPrevented]));
      const clickEvent = new Event('click');
      const clickResult = button.dispatchEvent(clickEvent);

      window.onmessage = () => 0;
      const zeroEvent = new Event('message');
      const zeroResult = window.dispatchEvent(zeroEvent);
      window.onmessage = null;
      return [
        [messageResult, messageEvent.cancelable, messageEvent.defaultPrevented, windowOrder],
        [clickResult, clickEvent.cancelable, clickEvent.defaultPrevented, attributeCalls, attributeOrder],
        [zeroResult, zeroEvent.defaultPrevented],
      ];
    })()"#
}

pub(super) fn native_projected_window_document_event_handler_dispatch_script() -> &'static str {
    r#"(() => {
      const frame = document.getElementById('child');
      const frameDocument = frame.contentDocument;
      const frameWindow = frame.contentWindow;
      const events = [];
      const body = frameDocument.body;
      const load = function(event) {
        events.push(['frame-body-load', this === frameWindow, event.target === frameWindow, event.currentTarget === frameWindow]);
      };
      body.onload = load;
      const aliasesWindow = frameDocument.defaultView === frameWindow
        && frameWindow.onload === load && body.onload === load;
      frameWindow.dispatchEvent(new Event('load'));
      body.onload = null;
      frameDocument.onvisibilitychange = function(event) {
        events.push(['frame-document-visibility', this === frameDocument, event.target === frameDocument, event.currentTarget === frameDocument]);
      };
      frameDocument.dispatchEvent(new Event('visibilitychange'));
      frameDocument.onvisibilitychange = null;
      globalThis.projectedWindowAttributeCalls = [];
      globalThis.projectedWindowAttributeTarget = frameWindow;
      frameWindow.projectedWindowAttributeCalls = globalThis.projectedWindowAttributeCalls;
      frameWindow.projectedWindowAttributeTarget = frameWindow;
      body.setAttribute('onload', "globalThis.projectedWindowAttributeCalls.push([this === globalThis.projectedWindowAttributeTarget, event.currentTarget === globalThis.projectedWindowAttributeTarget])");
      const assignedContentAttributeHandler = typeof body.onload === 'function';
      const contentAttributeAliasesWindow = body.onload === frameWindow.onload;
      frameWindow.dispatchEvent(new Event('load'));
      const contentAttributeCalls = globalThis.projectedWindowAttributeCalls.slice();
      body.removeAttribute('onload');
      return [events, aliasesWindow, frameWindow.onload === null, body.onload === null,
        [assignedContentAttributeHandler, contentAttributeAliasesWindow, contentAttributeCalls,
          frameWindow.onload === null]];
    })()"#
}

pub(super) fn native_global_event_handler_dispatch_script() -> &'static str {
    r#"(() => {
      const first = document.getElementById('first');
      globalThis.commonEventHandlerEvents = [];
      first.onclick = function(event) {
        commonEventHandlerEvents.push([
          'click',
          this === event.currentTarget,
          event.target === event.currentTarget,
        ]);
      };
      first.oninput = function(event) {
        commonEventHandlerEvents.push([
          'input',
          this === event.currentTarget,
          event.target === event.currentTarget,
        ]);
      };
      first.dispatchEvent(new Event('click', { bubbles: true }));
      first.dispatchEvent(new Event('input', { bubbles: true }));
      first.onclick = null;
      first.oninput = null;
      return [commonEventHandlerEvents, first.onclick, first.oninput];
    })()"#
}

pub(super) fn native_projected_focus_handler_setup_script() -> &'static str {
    r#"(() => {
      const child = document.getElementById('child').contentDocument;
      const outer = child.getElementById('outer');
      const first = child.getElementById('first');
      const second = child.getElementById('second');
      globalThis.projectedFocusEvents = [];
      globalThis.projectedIdlFocusEvents = [];
      globalThis.projectedIdlClickEvents = [];
      const record = event => projectedFocusEvents.push([
        event.type,
        event.target.id,
        event.relatedTarget ? event.relatedTarget.id : null,
        event instanceof FocusEvent,
      ]);
      for (const type of ['blur', 'focus']) outer.addEventListener(type, record, true);
      for (const type of ['focusout', 'focusin']) outer.addEventListener(type, record);
      first.onclick = function(event) {
        projectedIdlClickEvents.push([
          this === event.currentTarget,
          event.target === event.currentTarget,
        ]);
      };
      first.onfocus = () => projectedIdlFocusEvents.push(['replaced']);
      first.onfocus = function(event) {
        projectedIdlFocusEvents.push([
          'focus-first',
          this === event.currentTarget,
          event.target === event.currentTarget,
        ]);
      };
      first.onblur = function(event) {
        projectedIdlFocusEvents.push([
          'blur-first',
          this === event.currentTarget,
          event.target === event.currentTarget,
        ]);
      };
      second.onfocus = function(event) {
        projectedIdlFocusEvents.push([
          'focus-second',
          this === event.currentTarget,
          event.target === event.currentTarget,
        ]);
      };
      second.onblur = function(event) {
        projectedIdlFocusEvents.push([
          'blur-second',
          this === event.currentTarget,
          event.target === event.currentTarget,
        ]);
      };
      return true;
    })()"#
}

pub(super) fn native_projected_global_handler_dispatch_script() -> &'static str {
    r#"(() => {
      const child = document.getElementById('child').contentDocument;
      const first = child.getElementById('first');
      globalThis.projectedIdlClickEvents = [];
      const handler = first.onclick;
      first.dispatchEvent(new Event('click', { bubbles: true }));
      first.onclick = null;
      return [typeof handler, first.onclick, projectedIdlClickEvents];
    })()"#
}

pub(super) fn native_projected_focus_handler_clear_script() -> &'static str {
    "(() => { const child = document.getElementById('child').contentDocument; const first = child.getElementById('first'); const second = child.getElementById('second'); first.onfocus = null; second.onblur = null; return [first.onfocus, second.onblur]; })()"
}
