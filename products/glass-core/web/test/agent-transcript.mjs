import assert from 'node:assert/strict';
import {transcriptEntries} from '../src/agent-transcript.js';
const entries=transcriptEntries([
 {k:'reasoning',t:'Read '},{k:'reasoning',t:'the project.'},
 {k:'call',id:'1',name:'read',args:'README.md'},
 {k:'tool',id:'1',t:'Project contents'},
 {k:'assistant',t:'All '},{k:'assistant',t:'done.'},
 {k:'custom',c:'provider/chat-reasoning',d:'PRIVATE_SENTINEL'}
].map(JSON.stringify).join('\n'));
assert.deepEqual(entries.map(e=>e.kind),['reasoning','tool','assistant']);
assert.equal(entries[0].text,'Read the project.');
assert.equal(entries[1].text,'README.md\nProject contents');
assert.equal(entries[1].finished,true);
assert.equal(entries[2].text,'All done.');
assert(!JSON.stringify(entries).includes('PRIVATE_SENTINEL'));
assert.equal(transcriptEntries('A plain transcript.')[0].text,'A plain transcript.\n');
assert.deepEqual(transcriptEntries('{\"k\":\"custom\",\"d\":\"PRIVATE'),[]);
console.log('PASS readable transcript fragments, grouped tool results, plain text fallback and private-record filtering');
