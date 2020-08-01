class AddDefaultValueBillsAgain < ActiveRecord::Migration[5.2]
  def change
    change_column :bills, :paid, :boolean, null: false, default: false
  end
end
