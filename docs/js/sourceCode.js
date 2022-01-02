document.addEventListener("DOMContentLoaded", function () {
  const $tabSource = document.querySelector("#source-tab");
  const $tabInfo = document.querySelector("#info-tab");
  const $tabReadme = document.querySelector("#readme-tab");
  const $tabTemplate = document.querySelector("#templateData-tab");
  const $tabTree = document.querySelector("#tree-tab");
  const $tabExample = document.querySelector("#example-tab");
  const $prismPre = document.querySelector("pre.compodoc-sourcecode");
  if ($tabSource && $prismPre) {
    ($prismCode = $prismPre.querySelector("code")),
      ($content = document.querySelector(".content")),
      (prismLinks = document.querySelectorAll(".link-to-prism"));

    for (let i = 0; i < prismLinks.length; i++) {
      prismLinks[i].addEventListener("click", linkToPrism, false);
    }

    function linkToPrism(event) {
      const targetLine = event.target.getAttribute("data-line");
      event.preventDefault();

      $prismPre.setAttribute("data-line", targetLine);
      Prism.highlightElement($prismCode, function () {});

      $tabSource.click();

      setTimeout(function () {
        const $prismHighlightLine = document.querySelector(".line-highlight");
        const top = parseInt(getComputedStyle($prismHighlightLine).top);
        $content.scrollTop = top;
      }, 500);
    }

    window.onhashchange = function (event) {
      switch (window.location.hash) {
        case "":
        case "#info":
          $tabInfo.click();
          break;
        case "#readme":
          $tabReadme.click();
          break;
        case "#source":
          $tabSource.click();
          break;
        case "#template":
          $tabTemplate.click();
          break;
        case "#dom-tree":
          $tabTree.click();
          break;
        case "#example":
          $tabExample.click();
          break;
      }
    };
  }
});
