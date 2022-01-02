document.addEventListener("DOMContentLoaded", function () {
  function htmlEntities(str) {
    return String(str)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function foundLazyModuleWithPath(path) {
    // path is like app/customers/customers.module#CustomersModule
    const split = path.split("#");
    const lazyModulePath = split[0];
    const lazyModuleName = split[1];
    return lazyModuleName;
  }

  function getBB(selection) {
    selection.each(function (d) {
      d.bbox = this.getBBox();
    });
  }

  let test_cases, test_case, test_case_num, engine;

  const tree = ROUTES_INDEX;

  function cleanStringChildren(obj) {
    for (const property in obj) {
      if (obj.hasOwnProperty(property)) {
        if (property === "children" && typeof obj[property] === "object") {
          for (let i = obj[property].length - 1; i >= 0; i--) {
            if (typeof obj[property][i] === "string") {
              obj[property].splice(i, 1);
            }
          }
        }
        if (typeof obj[property] === "object") {
          cleanStringChildren(obj[property]);
        }
      }
    }
  }
  cleanStringChildren(tree);

  engine = d3.layout.tree().setNodeSizes(true);

  engine.spacing(function (a, b) {
    return a.parent == b.parent ? 0 : engine.rootXSize();
  });

  engine.nodeSize(function (d) {
    return [
      document.getElementById(d.id).getBBox().height + 70,
      document.getElementById(d.id).getBBox().width + 30,
    ];
  });

  let nodes = d3.layout.hierarchy()(tree);

  const svg = d3.select("#body-routes").append("svg");
  const svg_g = svg.append("g");
  const svg_p = svg.append("g");
  let last_id = 0;

  const node = svg_g
    .selectAll(".node")
    .data(nodes, function (d) {
      return d.id || (d.id = ++last_id);
    })
    .enter()
    .append("g")
    .attr("class", "node");

  svg.attr("id", "main");

  svg_g.attr("transform", "translate(20,0)").attr("id", "main-group");

  svg_p.attr("transform", "translate(20,0)").attr("id", "paths");

  const infos_group = node.append("g").attr({
    id: function (d) {
      return d.id;
    },
    dx: 0,
    dy: 0,
  });

  // Node icon
  infos_group
    .append("text")
    .attr("font-family", "Ionicons")
    .attr("y", 5)
    .attr("x", 0)
    .attr("class", function (d) {
      return d.children || d._children ? "icon has-children" : "icon";
    })
    .attr("font-size", function (d) {
      return "15px";
    })
    .text(function (d) {
      return "\uf183";
    });

  // node infos
  infos_group
    .append("svg:text")
    .attr("x", function (d) {
      return 0;
    })
    .attr("y", function (d) {
      return 10;
    })
    .attr("dy", ".35em")
    .attr("class", "text")
    .attr("text-anchor", function (d) {
      return "start";
    })
    .html(function (d) {
      // if kind === module name + module
      // if kind === component component + path
      let _name = "";
      if (d.kind === "module") {
        if (d.module) {
          _name +=
            '<tspan x="0" dy="1.4em"><a href="./modules/' +
            d.module +
            '.html">' +
            d.module +
            "</a></tspan>";
          if (d.name) {
            _name += '<tspan x="0" dy="1.4em">' + d.name + "</tspan>";
          }
        } else {
          _name +=
            '<tspan x="0" dy="1.4em">' + htmlEntities(d.name) + "</tspan>";
        }
      } else if (d.kind === "component") {
        _name += '<tspan x="0" dy="1.4em">' + d.path + "</tspan>";
        _name +=
          '<tspan x="0" dy="1.4em"><a href="./components/' +
          d.component +
          '.html">' +
          d.component +
          "</a></tspan>";
        if (d.outlet) {
          _name +=
            '<tspan x="0" dy="1.4em">&lt;outlet&gt; : ' + d.outlet + "</tspan>";
        }
      } else {
        _name += '<tspan x="0" dy="1.4em">/' + d.path + "</tspan>";
        if (d.component) {
          _name +=
            '<tspan x="0" dy="1.4em"><a href="./components/' +
            d.component +
            '.html">' +
            d.component +
            "</a></tspan>";
        }
        if (d.loadChildren) {
          const moduleName = foundLazyModuleWithPath(d.loadChildren);
          _name +=
            '<tspan x="0" dy="1.4em"><a href="./modules/' +
            moduleName +
            '.html">' +
            moduleName +
            "</a></tspan>";
        }
        if (d.canActivate) {
          _name += '<tspan x="0" dy="1.4em">&#10003; canActivate</tspan>';
        }
        if (d.canDeactivate) {
          _name +=
            '<tspan x="0" dy="1.4em">&#215;&nbsp;&nbsp;canDeactivate</tspan>';
        }
        if (d.canActivateChild) {
          _name += '<tspan x="0" dy="1.4em">&#10003; canActivateChild</tspan>';
        }
        if (d.canLoad) {
          _name += '<tspan x="0" dy="1.4em">&#8594; canLoad</tspan>';
        }
        if (d.redirectTo) {
          _name +=
            '<tspan x="0" dy="1.4em">&rarr; ' + d.redirectTo + "</tspan>";
        }
        if (d.pathMatch) {
          _name += '<tspan x="0" dy="1.4em">&gt; ' + d.pathMatch + "</tspan>";
        }
        if (d.outlet) {
          _name +=
            '<tspan x="0" dy="1.4em">&lt;outlet&gt; : ' + d.outlet + "</tspan>";
        }
      }
      return _name;
    })
    .call(getBB);

  //
  // Node lazy loaded ?
  //
  infos_group
    .append("svg:text")
    .attr("y", function (d) {
      return 45;
    })
    .attr("x", function (d) {
      return -18;
    })
    .attr("font-family", "Ionicons")
    .attr("class", function (d) {
      return "icon";
    })
    .attr("font-size", function (d) {
      return "15px";
    })
    .text(function (d) {
      let _text = "";
      if (d.loadChildren) {
        _text = "\uf4c1";
      }
      if (d.guarded) {
        _text = "\uf1b0";
      }
      return _text;
    });

  // Node text background
  infos_group
    .insert("rect", "text")
    .attr("width", function (d) {
      return d.bbox.width;
    })
    .attr("height", function (d) {
      return d.bbox.height;
    })
    .attr("y", function (d) {
      return 15;
    })
    .style("fill", "white")
    .style("fill-opacity", 0.75);

  nodes = engine.nodes(tree);

  function node_extents(n) {
    return [n.x - n.x_size / 2, n.y, n.x + n.x_size / 2, n.y + n.y_size];
  }
  const root_extents = node_extents(nodes[0]);
  let xmin = root_extents[0];
  let ymin = root_extents[1];
  let xmax = root_extents[2];
  let ymax = root_extents[3];
  let area_sum = (xmax - xmin) * (ymax - ymin);
  let x_size_min = nodes[0].x_size;
  let y_size_min = nodes[0].y_size;

  nodes.slice(1).forEach(function (n) {
    const ne = node_extents(n);
    xmin = Math.min(xmin, ne[0]);
    ymin = Math.min(ymin, ne[1]);
    xmax = Math.max(xmax, ne[2]);
    ymax = Math.max(ymax, ne[3]);
    area_sum += (ne[2] - ne[0]) * (ne[3] - ne[1]);
    x_size_min = Math.min(x_size_min, n.x_size);
    y_size_min = Math.min(y_size_min, n.y_size);
  });

  const area_ave = area_sum / nodes.length;
  const scale = 80 / Math.sqrt(area_ave);

  function svg_x(node_y) {
    return node_y - ymin;
  }

  function svg_y(node_x) {
    return (node_x - xmin) * scale;
  }

  const nodebox_right_margin = Math.min(x_size_min * scale, 10);
  const nodebox_vertical_margin = Math.min(y_size_min * scale, 3);

  node.attr("transform", function (d) {
    return "translate(" + svg_x(d.y) + "," + svg_y(d.x) + ")";
  });

  const diagonal = d3.svg.diagonal().projection(function (d) {
    return [svg_x(d.y), svg_y(d.x)];
  });

  var links = engine.links(nodes);
  var links = svg_p
    .selectAll(".link")
    .data(links)
    .enter()
    .append("path")
    .attr("class", "link")
    .attr("d", diagonal);

  const _svg = document.getElementById("main");
  const main_g = _svg.childNodes[0];

  _svg.removeChild(main_g);
  _svg.appendChild(main_g);

  svg.attr({
    width: document.getElementById("main-group").getBBox().width + 30,
    height: document.getElementById("main-group").getBBox().height + 50,
  });
});
