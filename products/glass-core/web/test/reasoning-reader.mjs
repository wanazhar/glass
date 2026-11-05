import assert from 'node:assert/strict';
import {collectReasoning,readableReasoning} from '../src/reasoning-reader.js';
assert.equal(collectReasoning([{k:'reasoning',t:'Inspect the '},{k:'reasoning',t:'project.'},{k:'call'},{k:'tool'},{k:'reasoning',t:'Review results.'}]),'Inspect the project.\n\nReview results.');
assert.equal(collectReasoning([{k:'custom',c:'provider/chat-reasoning',d:'PRIVATE'},{k:'reasoning',t:'Public text.'}]),'Public text.');
const original='This is a complete sentence with enough words to exercise readable paragraph spacing. '.repeat(24),formatted=readableReasoning(original);
assert(formatted.includes('\n\n'));assert.equal(formatted.replace(/\s/g,''),original.replace(/\s/g,''));
const code='```text\n'+original+'\n```';assert.equal(readableReasoning(code),code);
console.log('PASS reasoning fragment assembly, phase boundaries, public-only content, paragraph fidelity and fenced code');
