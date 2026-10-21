"""Plan one joint census of owned, guest-visible bind sources; no host IO.

Callers must prove locality, creation ownership and no-follow root witnesses
around observation. Physical source paths and numeric witnesses stay private.
"""
import stat

from asb_material import MaterialError, path_name
from asb_material_binds import absolute, beneath


def minimal(values):
    result = []
    for value in sorted(set(values)):
        if not any(beneath(value, parent) for parent in result):
            result.append(value)
    return result


def relative(value):
    canonical = path_name(value)
    if not canonical or canonical != value:
        raise MaterialError("prepared bind graph needs canonical nonempty relative names")
    return canonical


def plan(views):
    if not isinstance(views,list) or len(views) > 32:
        raise MaterialError("prepared bind graph exceeds view bounds")
    groups, names = {}, set()
    for view in views:
        if not isinstance(view,dict) or set(view) != {"name","source","identity","exclusions"}:
            raise MaterialError("prepared bind graph view is malformed")
        name = relative(view["name"])
        source = absolute(view["source"])
        identity, exclusions = view["identity"],view["exclusions"]
        if (name in names or not isinstance(identity,list) or len(identity) != 3
                or any(type(value) is not int or value < 0 for value in identity)
                or identity[1] == 0 or identity[2] not in [stat.S_IFDIR,stat.S_IFREG]
                or not isinstance(exclusions,list) or len(exclusions) > 128):
            raise MaterialError("prepared bind graph identity or exclusions are invalid")
        names.add(name)
        exclusions = minimal([relative(value) for value in exclusions])
        group = groups.setdefault(source,{"identity":identity,"views":[]})
        if group["identity"] != identity:
            raise MaterialError("prepared bind source has contradictory witnesses")
        group["views"].append({"binding":name,"exclusions":exclusions})
    roots, bindings = [],[]
    ordered = sorted(groups.items(),key=lambda item:min(view["binding"] for view in item[1]["views"]))
    for index,(source,group) in enumerate(ordered,1):
        name = f"bind-{index:03}"
        candidates = {value for view in group["views"] for value in view["exclusions"]}
        hidden = minimal([value for value in candidates if all(
            any(beneath(value,parent) for parent in view["exclusions"]) for view in group["views"])])
        roots.append({"name":name,"path":source,"expected_identity":group["identity"],"exclusions":hidden})
        bindings.extend(dict(view,root=name) for view in group["views"])
    return {"roots":roots,"bindings":sorted(bindings,key=lambda view:view["binding"])}
