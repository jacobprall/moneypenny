class AddPaidBill < ActiveRecord::Migration[5.2]
  def change
    add_column :bills, :paid, :boolean, null: false
  end
end
