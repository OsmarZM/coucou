import test from 'node:test';
import assert from 'node:assert/strict';
import {mergePersonalPreferences} from '../../src/personal/core/preferences.ts';
import {ApprovalPresentation} from '../../src/personal/core/approval-presentation.ts';

test('personal preferences preserve official providers, monitor, shortcuts and extra settings',()=>{
 const official={chatProvider:'custom',customUrl:'http://localhost:8080',screen:'monitor-2',shortcuts:{chat:'Ctrl+Alt+C'},phoneLink:{enabled:false},model:'official-model'};
 const merged=mergePersonalPreferences(official,{visibilityMode:'always',userPinned:true,model:'old-model',chatProvider:'anthropic',screen:'primary'});
 for(const key of Object.keys(official)) assert.deepEqual(merged[key],official[key]);
 assert.equal(merged.userPinned,true);
 assert.equal(official.userPinned,undefined);
});
test('local approval waits behind another card and is shown once when unblocked',()=>{
 const p=new ApprovalPresentation();let opens=0;const open=()=>opens++;
 p.update('request-1',true);p.reveal(true,open);assert.equal(opens,0);
 p.reveal(false,open);p.reveal(false,open);assert.equal(opens,1);
 p.update('request-2',true);p.reveal(false,open);assert.equal(opens,2);
 p.update(null,true);assert.equal(p.pending,false);
});
test('an inactive local panel never opens an approval or grants permission',()=>{
 const p=new ApprovalPresentation();p.update('request-1',false);
 assert.equal(p.pending,false);p.reveal(false,()=>assert.fail('inactive card opened'));
});
