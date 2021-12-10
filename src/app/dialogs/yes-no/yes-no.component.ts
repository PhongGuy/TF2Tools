import { Component, Inject, OnInit } from '@angular/core';
import { MAT_DIALOG_DATA } from '@angular/material/dialog';
import { YesNo } from '../../models/yesNo';

@Component({
  selector: 'app-yes-no',
  templateUrl: './yes-no.component.html',
  styleUrls: ['./yes-no.component.scss']
})
export class YesNoComponent implements OnInit {

  constructor(
    @Inject(MAT_DIALOG_DATA) public data: YesNo
  ) { }

  ngOnInit(): void {
  }

}
