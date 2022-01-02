import { Component } from "@angular/core";
import { AppComponent } from "../app.component";

/**
 * Dashboard component
 */
@Component({
  selector: "app-dashboard",
  templateUrl: "./dashboard.component.html",
  styleUrls: ["./dashboard.component.scss"],
})
export class DashboardComponent {
  /**
   * Creates an instance of dashboard component.
   *
   * @param app AppComponent
   */
  constructor(public app: AppComponent) {}
}
