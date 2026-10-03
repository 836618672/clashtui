from pathlib import Path
from urllib.parse import unquote
import re, subprocess, collections
root=Path(__file__).resolve().parent.parent
files=[*sorted((root/'docs').rglob('*.md')),root/'README.md',root/'README_ZH.md',root/'CHANGELOG.md',root/'dockers/README.md']
errors=[];links=0;edges={};anchors={}
for p in files:
 text=p.read_text();visible=re.sub(r'```.*?```','',text,flags=re.S)
 slugs=[];counts=collections.Counter()
 for title in re.findall(r'^#{1,6} (.+)$',visible,re.M):
  title=re.sub(r'<[^>]+>','',title).lower()
  slug=re.sub(r'[^\w\- ]','',title).replace(' ','-')
  n=counts[slug];counts[slug]+=1;slugs.append(slug+(f'-{n}' if n else ''))
 anchors[p]=set(slugs)
for p in files:
 text=p.read_text();edges[p]=set()
 assert text.count('```')%2==0,p
 for url in re.findall(r'\]\(([^\s()]+)\)',text):
  if re.match(r'\w+:',url):continue
  path,sep,anchor=url.partition('#');target=(p.parent/unquote(path)).resolve() if path else p
  links+=1
  if not target.exists():errors.append((str(p.relative_to(root)),url,'missing file'));continue
  edges[p].add(target)
  if sep and target in anchors and unquote(anchor) not in anchors[target]:errors.append((str(p.relative_to(root)),url,'missing anchor'))
seen=set();pending=[root/'docs/README.md']
while pending:
 p=pending.pop()
 if p in seen:continue
 seen.add(p);pending.extend(edges.get(p,set())-seen)
unreachable=set((root/'docs').rglob('*.md'))-seen
for p in unreachable:errors.append((str(p.relative_to(root)),'','not reachable from index'))
manual=(root/'docs/testing/tui_web_testing_zh.md').read_text()
record=(root/'docs/testing/manual_test_record_zh.md').read_text()
assert len(re.findall(r'^### M\d\d：',manual,re.M))==30
assert len(re.findall(r'^\| H\d\d \|',record,re.M))==6
plan=(root/'docs/testing/README.md').read_text()
assert len(re.findall(r'^\| M\d\d ',plan,re.M))==30
assert len(re.findall(r'^\| H\d\d ',plan,re.M))==6
for language,code in re.findall(r'```([^\n]*)\n(.*?)```',manual+'\n'+plan,re.S):
 if language.strip()=='bash':
  check=subprocess.run(['bash','-n'],input=code,text=True,capture_output=True)
  assert check.returncode==0,check.stderr
for error in errors:print(error)
assert not errors, f'{len(errors)} documentation errors'
print(f'PASS: {len(files)} Markdown files; {links} local links and anchors; all docs reachable; 30 detailed cases and coverage rows; 6 human checks and records; Bash syntax')
