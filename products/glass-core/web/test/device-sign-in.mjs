import assert from 'node:assert/strict';
import {devicePollRequest} from '../src/settings.js';

const defaults={model:'claude-opus-4-5'};
assert.equal(devicePollRequest({attempt:'attempt',defaults},'revision'),null);
for(const model of ['', '  ', 'x'.repeat(257), 'bad\nmodel', 'é'.repeat(129)]){
  assert.equal(devicePollRequest({attempt:'attempt',model},'revision'),null);
}
assert.deepEqual(devicePollRequest({attempt:'attempt',model:' gpt-5-codex ',defaults},'revision'),{
  revision:'revision',action:'device-poll',args:'attempt',model:'gpt-5-codex',confirmed:true,
});
console.log('PASS device sign-in requires an explicit model and never reuses the global provider model');
