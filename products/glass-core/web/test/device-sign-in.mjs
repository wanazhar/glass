import assert from 'node:assert/strict';
import {confirmDeviceModel,devicePollRequest} from '../src/settings.js';

const defaults={model:'claude-opus-4-5'};
assert.equal(devicePollRequest({attempt:'attempt',defaults},'revision'),null);
for(const model of ['', '  ', 'x'.repeat(257), 'bad\nmodel', 'é'.repeat(129)]){
  const device={attempt:'attempt',model};
  assert.equal(confirmDeviceModel(device),false);
  assert.equal(devicePollRequest(device,'revision'),null);
}
const device={attempt:'attempt',model:'',defaults};
const saved=[];
const poll=()=>{
  const request=devicePollRequest(device,'revision');
  if(request)saved.push(request);
};
for(const draft of ['g','gpt-','gpt-5','gpt-5-codex']){
  device.model=draft;
  poll();
  assert.equal(saved.length,0,'approval must not complete sign-in while typing');
}
device.model=' gpt-5-codex ';
assert.equal(confirmDeviceModel(device),true);
device.model='an-unconfirmed-edit';
poll();
assert.deepEqual(saved,[{
  revision:'revision',action:'device-poll',args:'attempt',model:'gpt-5-codex',confirmed:true,
}]);
console.log('PASS device approval cannot save an unfinished model; only the explicitly confirmed snapshot is submitted');
