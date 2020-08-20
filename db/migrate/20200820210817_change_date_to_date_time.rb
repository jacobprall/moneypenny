class ChangeDateToDateTime < ActiveRecord::Migration[5.2]
  def change
    remove_column :transactions, :date 
    add_column :transactions, :date, :datetime, null: false
  end
end
