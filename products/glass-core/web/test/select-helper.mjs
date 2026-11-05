// Exercise the visible in-app menu, while retaining support for unenhanced fixtures.
export async function chooseOption(page,locator,choice){
  const native=await locator.evaluate(n=>n.tagName==='SELECT');
  if(native&&!await locator.locator('..').getByRole('combobox').count())return locator.selectOption(choice);
  const wrapper=locator.locator('..');
  const select=wrapper.locator('select'),trigger=wrapper.getByRole('combobox');
  const index=await select.evaluate((n,choice)=>[...n.options].findIndex(o=>typeof choice==='string'?o.value===choice:choice.value!==undefined?o.value===choice.value:o.label===choice.label),choice);
  if(index<0)throw Error('Missing option '+JSON.stringify(choice));
  const id=await trigger.getAttribute('aria-controls');await trigger.click();
  await page.locator('#'+id).locator('[role=option]').nth(index).click();
}
