"""Independent fixture AST/netlist checks for a single approved schematic value edit."""
import copy
from decimal import Decimal
import json
import re
import xml.etree.ElementTree as ET


def tree(data):
    text=data.decode('utf-8')
    tokens=re.findall(r'"(?:\\.|[^"\\])*"|\(|\)|[^\s()]+',text)
    stack=[]
    roots=[]
    for token in tokens:
        if token=='(':
            node=[]
            if stack: stack[-1].append(node)
            else: roots.append(node)
            stack.append(node)
        elif token==')':
            assert stack,'Unmatched schematic delimiter'
            stack.pop()
        else:
            assert stack,'Atom outside schematic root'
            if token.startswith('"'): atom=('quoted',json.loads(token))
            elif re.fullmatch(r'[-+]?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?',token): atom=Decimal(token)
            else: atom=token
            stack[-1].append(atom)
    assert not stack and len(roots)==1 and roots[0][0]=='kicad_sch'
    return roots[0]


def assert_only_value_changed(before,after,reference,value):
    expected=copy.deepcopy(tree(before))
    changed=0
    for node in expected[1:]:
        if isinstance(node,list) and node and node[0]=='symbol':
            properties=[child for child in node[1:] if isinstance(child,list) and child and child[0]=='property']
            if any(child[1:3]==[('quoted','Reference'),('quoted',reference)] for child in properties):
                for child in properties:
                    if child[1]==('quoted','Value'):
                        child[2]=('quoted',value)
                        changed+=1
    assert changed>0,'Target reference not found in fixture'
    assert tree(after)==expected,'Schematic content changed outside the approved Value field'


def assert_netlist(path,reference,value):
    root=ET.parse(path).getroot()
    selected=root.find(f"./components/comp[@ref='{reference}']/value")
    assert selected is not None and selected.text==value
