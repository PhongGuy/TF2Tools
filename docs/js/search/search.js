(function (compodoc) {
  const usePushState = typeof history.pushState !== "undefined";

  // DOM Elements
  const $body = $("body");
  let $searchResults;
  let $searchInput;
  let $searchList;
  let $searchTitle;
  let $searchResultsCount;
  let $searchQuery;
  let $mainContainer;
  let $xsMenu;

  // Throttle search
  function throttle(fn, wait) {
    let timeout;

    return function () {
      const ctx = this;
      const args = arguments;
      if (!timeout) {
        timeout = setTimeout(function () {
          timeout = undefined;
          fn.apply(ctx, args);
        }, wait);
      }
    };
  }

  function displayResults(res) {
    const noResults = res.count == 0;
    const groups = {};
    $searchResults.toggleClass("no-results", noResults);

    // Clear old results
    $searchList.empty();

    // Display title for research
    $searchResultsCount.text(res.count);
    $searchQuery.text(res.query);

    // Group result by context
    res.results.forEach(function (res) {
      const context = res.title.split(" - ")[0];
      if (typeof groups[context] === "undefined") {
        groups[context] = {
          results: [res],
        };
      } else {
        groups[context].results.push(res);
      }
    });

    const sortedGroups = Object.keys(groups).sort();

    for (let i = 0; i < sortedGroups.length; i++) {
      const property = sortedGroups[i];

      const $li = $("<li>", {
        class: "search-results-group",
      });
      let finalPropertyLabel = "";
      const propertyLabels = property.split("-");

      if (
        propertyLabels.length === 2 &&
        propertyLabels[0] !== "miscellaneous" &&
        propertyLabels[0] !== "additional"
      ) {
        finalPropertyLabel =
          propertyLabels[0].charAt(0).toUpperCase() +
          propertyLabels[0].substring(1) +
          " - " +
          propertyLabels[1].charAt(0).toUpperCase() +
          propertyLabels[1].substring(1) +
          " (" +
          groups[property].results.length +
          ")";
      } else if (propertyLabels[0] === "additional") {
        finalPropertyLabel =
          "Additional pages" + " (" + groups[property].results.length + ")";
      } else {
        finalPropertyLabel =
          propertyLabels[0].charAt(0).toUpperCase() +
          propertyLabels[0].substring(1) +
          " (" +
          groups[property].results.length +
          ")";
      }
      const $groupTitle = $("<h3>", {
        text: finalPropertyLabel,
      });
      $groupTitle.appendTo($li);

      var $ulResults = $("<ul>", {
        class: "search-results-list",
      });

      groups[property].results.forEach(function (res) {
        let link = "";
        const $liResult = $("<li>", {
          class: "search-results-item",
        });
        switch (COMPODOC_CURRENT_PAGE_DEPTH) {
          case 0:
            link = "./";
            break;
          case 1:
          case 2:
          case 3:
          case 4:
          case 5:
            link = "../".repeat(COMPODOC_CURRENT_PAGE_DEPTH);
            break;
        }
        const finalResLabel =
          res.title.split(" - ")[1].charAt(0).toUpperCase() +
          res.title.split(" - ")[1].substring(1);
        const $link = $("<a>", {
          href: link + res.url,
          text: finalResLabel,
        });
        $link.appendTo($liResult);
        $liResult.appendTo($ulResults);
      });
      $ulResults.appendTo($li);

      $li.appendTo($searchList);
    }
  }

  function launchSearch(q) {
    $body.addClass("with-search");

    if ($xsMenu.css("display") === "block") {
      $mainContainer.css("height", "calc(100% - 100px)");
      $mainContainer.css("margin-top", "100px");
    }

    throttle(
      compodoc.search.query(q, 0, MAX_SEARCH_RESULTS).then(function (results) {
        displayResults(results);
      }),
      1000
    );
  }

  function closeSearch() {
    $body.removeClass("with-search");
    if ($xsMenu.css("display") === "block") {
      $mainContainer.css("height", "calc(100% - 50px)");
      $mainContainer.css("margin-top", "50px");
    }
  }

  function bindMenuButton() {
    document.getElementById("btn-menu").addEventListener("click", function () {
      if ($xsMenu.css("display") === "none") {
        $body.removeClass("with-search");
        $mainContainer.css("height", "calc(100% - 50px)");
        $mainContainer.css("margin-top", "50px");
      }
      $.each($searchInputs, function (index, item) {
        var item = $(item);
        item.val("");
      });
    });
  }

  function bindSearch() {
    // Bind DOM
    $searchInputs = $("#book-search-input input");

    $searchResults = $(".search-results");
    $searchList = $searchResults.find(".search-results-list");
    $searchTitle = $searchResults.find(".search-results-title");
    $searchResultsCount = $searchTitle.find(".search-results-count");
    $searchQuery = $searchTitle.find(".search-query");
    $mainContainer = $(".container-fluid");
    $xsMenu = $(".xs-menu");

    // Launch query based on input content
    function handleUpdate(item) {
      const q = item.val();

      if (q.length == 0) {
        closeSearch();
        window.location.href = window.location.href.replace(
          window.location.search,
          ""
        );
      } else {
        launchSearch(q);
      }
    }

    // Detect true content change in search input
    let propertyChangeUnbound = false;

    $.each($searchInputs, function (index, item) {
      var item = $(item);
      // HTML5 (IE9 & others)
      item.on("input", function (e) {
        // Unbind propertychange event for IE9+
        if (!propertyChangeUnbound) {
          $(this).unbind("propertychange");
          propertyChangeUnbound = true;
        }

        handleUpdate($(this));
      });
      // Workaround for IE < 9
      item.on("propertychange", function (e) {
        if (e.originalEvent.propertyName == "value") {
          handleUpdate($(this));
        }
      });
      // Push to history on blur
      item.on("blur", function (e) {
        // Update history state
        if (usePushState) {
          const uri = updateQueryString("q", $(this).val());
          if ($(this).val() !== "") {
            history.pushState({ path: uri }, null, uri);
          }
        }
      });
    });
  }

  function launchSearchFromQueryString() {
    const q = getParameterByName("q");
    if (q && q.length > 0) {
      // Update search inputs
      $.each($searchInputs, function (index, item) {
        var item = $(item);
        item.val(q);
      });
      // Launch search
      launchSearch(q);
    }
  }

  compodoc.addEventListener(compodoc.EVENTS.SEARCH_READY, function (event) {
    bindSearch();

    bindMenuButton();

    launchSearchFromQueryString();
  });

  function getParameterByName(name) {
    const url = window.location.href;
    name = name.replace(/[\[\]]/g, "\\$&");
    const regex = new RegExp("[?&]" + name + "(=([^&#]*)|&|#|$)", "i");
    const results = regex.exec(url);
    if (!results) return null;
    if (!results[2]) return "";
    return decodeURIComponent(results[2].replace(/\+/g, " "));
  }

  function updateQueryString(key, value) {
    value = encodeURIComponent(value);

    let url = window.location.href;
    const re = new RegExp("([?&])" + key + "=.*?(&|#|$)(.*)", "gi");
    let hash;

    if (re.test(url)) {
      if (typeof value !== "undefined" && value !== null) {
        return url.replace(re, "$1" + key + "=" + value + "$2$3");
      } else {
        hash = url.split("#");
        url = hash[0].replace(re, "$1$3").replace(/(&|\?)$/, "");
        if (typeof hash[1] !== "undefined" && hash[1] !== null) {
          url += "#" + hash[1];
        }
        return url;
      }
    } else {
      if (typeof value !== "undefined" && value !== null) {
        const separator = url.indexOf("?") !== -1 ? "&" : "?";
        hash = url.split("#");
        url = hash[0] + separator + key + "=" + value;
        if (typeof hash[1] !== "undefined" && hash[1] !== null) {
          url += "#" + hash[1];
        }
        return url;
      } else {
        return url;
      }
    }
  }
})(compodoc);
