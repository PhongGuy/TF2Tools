import { Component, OnInit } from '@angular/core';
import { AppComponent } from '../../app.component';

@Component({
  selector: 'app-settings',
  templateUrl: './settings.component.html',
  styleUrls: ['./settings.component.scss']
})
export class SettingsComponent implements OnInit {

  constructor(
    public app: AppComponent
  ) { }

  ngOnInit(): void {
  }

  changelibrary(event: Event) {
    const target = event.target as HTMLInputElement;
    const files: File[] = Array.from(target.files);
  }
}
