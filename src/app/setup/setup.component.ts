import { Component, ElementRef, OnInit, ViewChild } from "@angular/core";
import { Router } from "@angular/router";
import { AppComponent } from "../app.component";
import { ElectronService } from "../core/services";
import { SnackService } from "../services/snack.service";

/**
 * Setup
 */
@Component({
  selector: "app-setup",
  templateUrl: "./setup.component.html",
  styleUrls: ["./setup.component.scss"],
})
export class SetupComponent implements OnInit {
  /**
   * path `ElementRef`
   */
  @ViewChild("path") path: ElementRef;
  /**
   * Sof setup component
   */
  private s = "Team Fortress 2\\tf\\custom";

  /**
   * Creates an instance of setup component.
   *
   * @param electron
   * @param app
   * @param snack
   * @param router
   */
  constructor(
    private electron: ElectronService,
    private app: AppComponent,
    private snack: SnackService,
    private router: Router
  ) {}

  /**
   * on init
   */
  ngOnInit(): void {}

  /**
   * Gets folder
   *
   * @param event
   */
  getFolder(event: Event): void {
    // reset form
    this.app.loading = true;
    this.path.nativeElement.value = null;
    const target = event.target as HTMLInputElement;
    const files: File[] = Array.from(target.files);
    if (files.length > 0) {
      let path = files[0].path;
      if (path.includes(this.s)) {
        this.app.loading = false;
        const split = path.split(this.s);
        path = `${split[0]}\\${this.s}`;
        this.path.nativeElement.value = `${split[0]}\\${this.s}`;
        this.app.settings.customPath = path;
        this.app.settingsUpdate.next(this.app.settings);
        this.router.navigate(["dashboard/hud"]);
      } else {
        this.snack.show("This is not a tf2 custom folder", null, 5000);
      }
    }
    this.app.loading = false;
  }
}
